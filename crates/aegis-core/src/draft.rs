//! Editable, secret-free representation of a profile for the UI editor.
//!
//! The UI never sends or receives key material. Private and preshared keys
//! can only be kept, generated, or removed; pasting keys is done by importing
//! a `.conf` file, which is read by the Rust side directly.

use std::net::IpAddr;

use aegis_config::{
    ConfigError, Endpoint, Interface, Issue, Peer, PublicKey, SecretKey, Severity, WgConfig,
};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Keep,
    Generate,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerDraft {
    pub public_key: String,
    pub preshared_key: KeyAction,
    /// Read-only hint for the UI.
    #[serde(default)]
    pub has_preshared_key: bool,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileDraft {
    pub name: String,
    pub private_key: KeyAction,
    /// Read-only: derived from the current private key.
    #[serde(default)]
    pub public_key: Option<String>,
    pub addresses: Vec<String>,
    pub dns: Vec<String>,
    pub mtu: Option<u16>,
    pub listen_port: Option<u16>,
    pub peers: Vec<PeerDraft>,
}

pub const MAX_NAME_LEN: usize = 64;

pub fn clean_name(name: &str) -> Result<String, String> {
    let n: String = name
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_owned();
    if n.is_empty() {
        return Err("name cannot be empty".into());
    }
    if n.chars().count() > MAX_NAME_LEN {
        return Err(format!("name must be at most {MAX_NAME_LEN} characters"));
    }
    Ok(n)
}

impl ProfileDraft {
    pub fn from_config(name: &str, cfg: &WgConfig) -> Self {
        let i = &cfg.interface;
        Self {
            name: name.to_owned(),
            private_key: KeyAction::Keep,
            public_key: Some(i.private_key.public_key().to_string()),
            addresses: i.addresses.iter().map(ToString::to_string).collect(),
            dns: i
                .dns
                .iter()
                .map(ToString::to_string)
                .chain(i.dns_search.iter().cloned())
                .collect(),
            mtu: i.mtu,
            listen_port: i.listen_port,
            peers: cfg
                .peers
                .iter()
                .map(|p| PeerDraft {
                    public_key: p.public_key.to_string(),
                    preshared_key: KeyAction::Keep,
                    has_preshared_key: p.preshared_key.is_some(),
                    endpoint: p
                        .endpoint
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    allowed_ips: p.allowed_ips.iter().map(ToString::to_string).collect(),
                    persistent_keepalive: p.persistent_keepalive,
                })
                .collect(),
        }
    }

    /// Template for a brand-new profile with a fresh key pair.
    pub fn blank() -> Self {
        Self {
            name: "New profile".into(),
            private_key: KeyAction::Generate,
            public_key: None,
            addresses: vec![],
            dns: vec![],
            mtu: None,
            listen_port: None,
            peers: vec![PeerDraft {
                public_key: String::new(),
                preshared_key: KeyAction::Remove,
                has_preshared_key: false,
                endpoint: String::new(),
                allowed_ips: vec!["0.0.0.0/0".into(), "::/0".into()],
                persistent_keepalive: Some(25),
            }],
        }
    }

    /// Build a config from this draft, taking kept secrets from `existing`.
    pub fn apply(&self, existing: Option<&WgConfig>) -> Result<WgConfig, ConfigError> {
        let mut issues = Vec::new();
        let mut err = |field: &str, msg: String| {
            issues.push(Issue {
                severity: Severity::Error,
                line: None,
                field: Some(field.into()),
                message: msg,
            })
        };
        if let Err(m) = clean_name(&self.name) {
            err("Name", m);
        }

        let gen_key = || SecretKey::generate().ok();
        let private_key = match (self.private_key, existing) {
            (KeyAction::Keep, Some(c)) => Some(c.interface.private_key.clone()),
            (KeyAction::Generate, _) => gen_key(),
            _ => None,
        };
        if private_key.is_none() {
            err(
                "PrivateKey",
                "a private key is required; choose Generate".into(),
            );
        }

        let mut addresses = Vec::new();
        for (n, a) in nonempty(&self.addresses).enumerate() {
            match a
                .parse::<IpNet>()
                .ok()
                .or_else(|| a.parse::<IpAddr>().ok().map(IpNet::from))
            {
                Some(v) => addresses.push(v),
                None => err(
                    "Address",
                    format!(
                        "entry {} is not a valid IP address with optional /prefix",
                        n + 1
                    ),
                ),
            }
        }
        let (mut dns, mut dns_search) = (Vec::new(), Vec::new());
        for d in nonempty(&self.dns) {
            match d.parse::<IpAddr>() {
                Ok(ip) => dns.push(ip),
                Err(_) => dns_search.push(d.to_ascii_lowercase()),
            }
        }

        let mut peers = Vec::new();
        for (n, p) in self.peers.iter().enumerate() {
            let n = n + 1;
            let public_key: Option<PublicKey> = p.public_key.trim().parse().ok();
            if public_key.is_none() {
                err(
                    "PublicKey",
                    format!("peer {n}: not a valid WireGuard public key"),
                );
            }
            let old_psk = existing
                .and_then(|c| c.peers.iter().find(|op| Some(op.public_key) == public_key))
                .and_then(|op| op.preshared_key.clone());
            let preshared_key = match p.preshared_key {
                KeyAction::Keep => old_psk,
                KeyAction::Generate => gen_key(),
                KeyAction::Remove => None,
            };
            let endpoint = match p.endpoint.trim() {
                "" => None,
                s => match s.parse::<Endpoint>() {
                    Ok(e) => Some(e),
                    Err(m) => {
                        err("Endpoint", format!("peer {n}: {m}"));
                        None
                    }
                },
            };
            let mut allowed_ips = Vec::new();
            for (k, a) in nonempty(&p.allowed_ips).enumerate() {
                match a
                    .parse::<IpNet>()
                    .ok()
                    .or_else(|| a.parse::<IpAddr>().ok().map(IpNet::from))
                {
                    Some(v) => allowed_ips.push(v),
                    None => err(
                        "AllowedIPs",
                        format!("peer {n}: entry {} is not a valid network (CIDR)", k + 1),
                    ),
                }
            }
            if let Some(pk) = public_key {
                peers.push(Peer {
                    public_key: pk,
                    preshared_key,
                    endpoint,
                    allowed_ips,
                    persistent_keepalive: p.persistent_keepalive.filter(|k| *k > 0),
                });
            }
        }

        let Some(private_key) = private_key else {
            return Err(ConfigError { issues });
        };
        let cfg = WgConfig {
            interface: Interface {
                private_key,
                addresses,
                dns,
                dns_search,
                mtu: self.mtu,
                listen_port: self.listen_port,
            },
            peers,
        };
        issues.extend(aegis_config::validate(&cfg));
        if issues.is_empty() {
            Ok(cfg)
        } else {
            Err(ConfigError { issues })
        }
    }
}

fn nonempty(v: &[String]) -> impl Iterator<Item = &str> {
    v.iter().map(|s| s.trim()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "[Interface]\nPrivateKey = dwdtCnMYpX08FsFyUbJmRd9ML4frwJkqsXf7pR25LCo=\nAddress = 10.0.0.2/32\n[Peer]\nPublicKey = 3p7bfXt9wbTTW2HC7OQ1Nz+DQ8hbeGdNrfx+FG+IK08=\nPresharedKey = MFujEGz6ut24WLYNi1ay+2fdQM2dc66lyn8CjB3arsM=\nEndpoint = 192.0.2.1:51820\nAllowedIPs = 10.0.0.0/24\n";

    #[test]
    fn keep_preserves_secrets_and_roundtrips() {
        let cfg = aegis_config::parse(CONF).unwrap();
        let d = ProfileDraft::from_config("x", &cfg);
        assert_eq!(d.apply(Some(&cfg)).unwrap(), cfg);
        // Draft serialization carries no secrets.
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("dwdtCnMY") && !json.contains("MFujEGz6"));
    }

    #[test]
    fn generate_and_remove() {
        let cfg = aegis_config::parse(CONF).unwrap();
        let mut d = ProfileDraft::from_config("x", &cfg);
        d.private_key = KeyAction::Generate;
        d.peers[0].preshared_key = KeyAction::Remove;
        let out = d.apply(Some(&cfg)).unwrap();
        assert_ne!(out.interface.private_key, cfg.interface.private_key);
        assert!(out.peers[0].preshared_key.is_none());
    }

    #[test]
    fn reports_all_field_errors() {
        let mut d = ProfileDraft::blank();
        d.name = " ".into();
        d.addresses = vec!["nope".into()];
        d.peers[0].endpoint = "host".into();
        let e = d.apply(None).unwrap_err();
        let fields: Vec<_> = e.issues.iter().filter_map(|i| i.field.clone()).collect();
        for f in ["Name", "Address", "PublicKey", "Endpoint"] {
            assert!(fields.iter().any(|x| x == f), "{f} missing from {fields:?}");
        }
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Home\u{7}  ").unwrap(), "Home");
        assert!(clean_name(&"x".repeat(65)).is_err());
    }
}
