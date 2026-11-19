//! Secret-free server metadata. A server IS a profile; endpoints/routing live in
//! its existing summary and credentials remain exclusively with VpnBackend.
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{CoreError, ProfileId, ProfileMeta, Result, Settings};

pub const HEALTH_MAX_AGE_MS: u64 = 5 * 60 * 1000;
pub const FAILURE_COOLDOWN_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Country {
    pub code: String,
    pub name: String,
}

pub fn countries() -> &'static [Country] {
    static COUNTRIES: OnceLock<Vec<Country>> = OnceLock::new();
    COUNTRIES.get_or_init(|| {
        serde_json::from_str(include_str!("../data/countries.json"))
            .expect("bundled ISO country data")
    })
}

pub fn country(code: &str) -> Option<&'static Country> {
    countries().iter().find(|c| c.code == code)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub country_code: String,
    pub city: Option<String>,
    pub region: Option<String>,
    pub time_zone: Option<String>,
}

impl Location {
    pub fn validate(&mut self) -> Result<()> {
        self.country_code = self.country_code.trim().to_ascii_uppercase();
        if country(&self.country_code).is_none() {
            return Err(CoreError::invalid("choose a valid ISO 3166-1 country"));
        }
        for text in [&mut self.city, &mut self.region, &mut self.time_zone] {
            if let Some(value) = text {
                let value = value.trim();
                if value.chars().count() > 100 || value.chars().any(char::is_control) {
                    return Err(CoreError::invalid(
                        "location fields must be at most 100 characters without control characters",
                    ));
                }
                *text = (!value.is_empty()).then(|| value.to_owned());
            }
        }
        Ok(())
    }
}

/// Proton VPN writes the server label (`# NL-FREE#1`, `# US#42`) as a comment
/// above the peer. Read it so an imported config lands under its country's
/// full name. The label is a hint from the file, not verified geography.
pub fn provider_label(text: &str) -> Option<(String, Location)> {
    let mut in_peer = false;
    for line in text.lines().map(str::trim) {
        if line.eq_ignore_ascii_case("[peer]") {
            in_peer = true;
            continue;
        }
        if !in_peer {
            continue;
        }
        let Some(label) = line.strip_prefix('#').map(str::trim) else {
            continue;
        };
        let (cc, rest) = label.split_once(['-', '#'])?;
        let valid_rest = rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '-'))
            && rest.chars().any(|c| c.is_ascii_digit())
            && label.len() <= 24;
        if cc.len() == 2 && cc.chars().all(|c| c.is_ascii_uppercase()) && valid_rest {
            country(cc)?;
            let location = Location {
                country_code: cc.to_owned(),
                city: None,
                region: None,
                time_zone: None,
            };
            return Some((label.to_owned(), location));
        }
        return None;
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerMetadata {
    pub location: Option<Location>,
    pub favorite: bool,
    pub enabled: bool,
    /// Generated local identities must be registered with the remote operator.
    pub registration_pending: bool,
    pub last_connected_ms: Option<u64>,
    pub last_attempt_ms: Option<u64>,
    pub last_failure_ms: Option<u64>,
    pub last_response_ms: Option<u64>,
}

impl Default for ServerMetadata {
    fn default() -> Self {
        Self {
            location: None,
            favorite: false,
            enabled: true,
            registration_pending: false,
            last_connected_ms: None,
            last_attempt_ms: None,
            last_failure_ms: None,
            last_response_ms: None,
        }
    }
}

/// Only editable metadata crosses IPC; callers cannot forge success history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerUpdate {
    pub location: Option<Location>,
    pub favorite: bool,
    pub enabled: bool,
    pub registration_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reachability {
    Unknown,
    Responded,
    NoResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServerHealth {
    pub id: ProfileId,
    pub reachability: Reachability,
    pub observed_ms: Option<u64>,
    /// None until a supported RTT measurement is implemented. Activation time
    /// and WireGuard handshake age must never be presented as latency.
    pub measured_rtt_ms: Option<u32>,
    pub last_handshake_ms: Option<u64>,
}

pub fn recent(at: Option<u64>, now: u64) -> bool {
    at.is_some_and(|at| at <= now && now - at <= HEALTH_MAX_AGE_MS)
}

pub fn eligible(p: &ProfileMeta) -> bool {
    p.server.enabled && !p.server.registration_pending && !p.summary.endpoints.is_empty()
}

#[derive(Debug, Clone, Serialize)]
pub struct QuickConnectChoice {
    pub id: ProfileId,
    pub reason: String,
}

/// Most backup servers tried in one fallback round.
pub const MAX_FALLBACKS: usize = 3;

/// Other usable servers in the same country as `failed`, best first: recent
/// authenticated response, then most recently connected, then by name.
/// Servers that failed in the last five minutes are skipped.
pub fn fallback_candidates(
    profiles: &[ProfileMeta],
    failed: ProfileId,
    now: u64,
) -> Vec<ProfileId> {
    let Some(code) = profiles
        .iter()
        .find(|p| p.id == failed)
        .and_then(|p| p.server.location.as_ref())
        .map(|l| l.country_code.clone())
    else {
        return vec![];
    };
    let mut c: Vec<_> = profiles
        .iter()
        .filter(|p| {
            p.id != failed
                && eligible(p)
                && p.server
                    .location
                    .as_ref()
                    .is_some_and(|l| l.country_code == code)
                && !p
                    .server
                    .last_failure_ms
                    .is_some_and(|at| at > now || now - at < FAILURE_COOLDOWN_MS)
        })
        .collect();
    c.sort_by(|a, b| {
        let rank = |p: &ProfileMeta| {
            (
                recent(p.server.last_response_ms, now),
                p.server.last_connected_ms.unwrap_or(0),
            )
        };
        rank(b)
            .cmp(&rank(a))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.id.cmp(&b.id))
    });
    c.into_iter().take(MAX_FALLBACKS).map(|p| p.id).collect()
}

/// No probes: recent authenticated responses, preferred profile, favorites,
/// last successful connection, then stable name/id order. Failed attempts cool
/// down for five minutes. RTT is unknown, so this never claims highest speed.
pub fn quick_connect(
    profiles: &[ProfileMeta],
    settings: &Settings,
    now: u64,
) -> Result<QuickConnectChoice> {
    let mut candidates: Vec<_> = profiles
        .iter()
        .filter(|p| {
            eligible(p)
                && settings.quick_connect_country.as_ref().is_none_or(|code| {
                    p.server
                        .location
                        .as_ref()
                        .is_some_and(|l| &l.country_code == code)
                })
                && !p
                    .server
                    .last_failure_ms
                    .is_some_and(|at| at > now || now - at < FAILURE_COOLDOWN_MS)
        })
        .collect();
    candidates.sort_by(|a, b| {
        let rank = |p: &ProfileMeta| {
            (
                recent(p.server.last_response_ms, now),
                settings.preferred_profile == Some(p.id),
                p.server.favorite
                    || p.server
                        .location
                        .as_ref()
                        .is_some_and(|l| settings.favorite_countries.contains(&l.country_code)),
                p.server.last_connected_ms.unwrap_or(0),
            )
        };
        rank(b)
            .cmp(&rank(a))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.id.cmp(&b.id))
    });
    let p = candidates
        .first()
        .ok_or_else(|| CoreError::invalid("No configured VPN servers are currently available."))?;
    Ok(QuickConnectChoice {
        id: p.id,
        reason: if recent(p.server.last_response_ms, now) {
            "Selected a server with a recent authenticated response"
        } else if settings.preferred_profile == Some(p.id) {
            "Selected your preferred server; current reachability is unknown"
        } else {
            "Selected an eligible configured server; current reachability and latency are unknown"
        }
        .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_provider_label() {
        let conf =
            "[Interface]\n# Key for x\nPrivateKey = a\n\n[Peer]\n# NL-FREE#1\nPublicKey = b\n";
        let (label, loc) = provider_label(conf).unwrap();
        assert_eq!(label, "NL-FREE#1");
        assert_eq!(loc.country_code, "NL");
        assert!(provider_label("[Interface]\n# US-FREE#155\n[Peer]\nPublicKey = b\n").is_none());
        assert!(provider_label("[Peer]\n# ZZ-FREE#1\n").is_none());
        assert!(provider_label("[Peer]\n# hello world\n").is_none());
    }

    #[test]
    fn iso_dataset_and_validation() {
        assert_eq!(countries().len(), 249);
        for code in ["SG", "JP", "DE", "TH", "US", "AQ"] {
            assert!(country(code).is_some());
        }
        let mut l = Location {
            country_code: " sg ".into(),
            city: Some(" Singapore ".into()),
            region: Some(" ".into()),
            time_zone: None,
        };
        l.validate().unwrap();
        assert_eq!(l.country_code, "SG");
        assert_eq!(l.city.as_deref(), Some("Singapore"));
        assert!(l.region.is_none());
        l.country_code = "ZZ".into();
        assert!(l.validate().is_err());
        l.country_code = "US".into();
        l.city = Some("a\nb".into());
        assert!(l.validate().is_err());
    }

    #[test]
    fn samples_expire_and_future_samples_are_not_fresh() {
        assert!(recent(Some(1000), 2000));
        assert!(!recent(Some(1000), HEALTH_MAX_AGE_MS + 1001));
        assert!(!recent(Some(2000), 1000));
        assert!(!recent(None, 1000));
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::ProfileSummary;

    fn profile(name: &str, code: &str) -> ProfileMeta {
        let cfg =
            aegis_config::parse(include_str!("../../../tests/fixtures/valid/split.conf")).unwrap();
        ProfileMeta {
            id: ProfileId::new(),
            name: name.into(),
            created_ms: 0,
            updated_ms: 0,
            summary: ProfileSummary::from_config(&cfg),
            server: ServerMetadata {
                location: Some(Location {
                    country_code: code.into(),
                    city: None,
                    region: None,
                    time_zone: None,
                }),
                ..Default::default()
            },
        }
    }

    #[test]
    fn quick_connect_uses_evidence_preferences_country_and_failure_cooldown() {
        let now = 1_000_000;
        let mut a = profile("A simulated", "SG");
        let mut b = profile("B simulated", "DE");
        let settings = Settings {
            preferred_profile: Some(a.id),
            ..Default::default()
        };
        assert_eq!(
            quick_connect(&[b.clone(), a.clone()], &settings, now)
                .unwrap()
                .id,
            a.id
        );
        b.server.last_response_ms = Some(now - 1000);
        assert_eq!(
            quick_connect(&[a.clone(), b.clone()], &settings, now)
                .unwrap()
                .id,
            b.id
        );
        b.server.last_failure_ms = Some(now);
        assert_eq!(
            quick_connect(&[a.clone(), b.clone()], &settings, now)
                .unwrap()
                .id,
            a.id
        );
        b.server.last_failure_ms = Some(now - FAILURE_COOLDOWN_MS);
        assert_eq!(
            quick_connect(&[a.clone(), b.clone()], &settings, now)
                .unwrap()
                .id,
            b.id
        );
        let settings = Settings {
            quick_connect_country: Some("SG".into()),
            ..settings
        };
        assert_eq!(
            quick_connect(&[a.clone(), b.clone()], &settings, now)
                .unwrap()
                .id,
            a.id
        );
        a.server.enabled = false;
        assert!(quick_connect(&[a, b], &settings, now).is_err());
    }

    #[test]
    fn expired_responses_do_not_override_favorites_and_pending_registration_is_excluded() {
        let now = HEALTH_MAX_AGE_MS + 10;
        let mut a = profile("A simulated", "SG");
        let mut b = profile("B simulated", "DE");
        a.server.last_response_ms = Some(1);
        b.server.favorite = true;
        assert_eq!(
            quick_connect(&[a.clone(), b.clone()], &Settings::default(), now)
                .unwrap()
                .id,
            b.id
        );
        b.server.registration_pending = true;
        assert_eq!(
            quick_connect(&[a.clone(), b], &Settings::default(), now)
                .unwrap()
                .id,
            a.id
        );
        a.server.last_failure_ms = Some(now + 1);
        assert!(quick_connect(&[a], &Settings::default(), now).is_err());
    }
}
