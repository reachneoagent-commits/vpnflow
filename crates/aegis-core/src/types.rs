//! Domain types shared by the engine, backends and the desktop UI.
//! Nothing in this module may hold secret key material.

use std::time::{SystemTime, UNIX_EPOCH};

use aegis_config::{Endpoint, RouteKind, WgConfig};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ProfileId(pub Uuid);

impl ProfileId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Non-secret facts about a profile, cached in app storage so listing
/// profiles never needs to touch the secret store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSummary {
    pub interface_public_key: String,
    pub addresses: Vec<String>,
    pub dns: Vec<String>,
    pub endpoints: Vec<String>,
    pub allowed_ips: Vec<String>,
    pub route_kind: RouteKind,
    pub peer_count: usize,
    pub mtu: Option<u16>,
    pub has_preshared_key: bool,
    pub warnings: Vec<String>,
}

impl ProfileSummary {
    pub fn from_config(cfg: &WgConfig) -> Self {
        let i = &cfg.interface;
        Self {
            interface_public_key: i.private_key.public_key().to_string(),
            addresses: i.addresses.iter().map(ToString::to_string).collect(),
            dns: i
                .dns
                .iter()
                .map(ToString::to_string)
                .chain(i.dns_search.iter().cloned())
                .collect(),
            endpoints: cfg
                .peers
                .iter()
                .filter_map(|p| p.endpoint.as_ref().map(Endpoint::to_string))
                .collect(),
            allowed_ips: cfg
                .peers
                .iter()
                .flat_map(|p| p.allowed_ips.iter().map(ToString::to_string))
                .collect(),
            route_kind: cfg.route_kind(),
            peer_count: cfg.peers.len(),
            mtu: i.mtu,
            has_preshared_key: cfg.peers.iter().any(|p| p.preshared_key.is_some()),
            warnings: aegis_config::warnings(cfg)
                .into_iter()
                .map(|w| w.to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub id: ProfileId,
    pub name: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub summary: ProfileSummary,
    /// Additive migration: older profiles remain usable with an unassigned location.
    #[serde(default)]
    pub server: crate::registry::ServerMetadata,
}

/// What the UI sees for a profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileView {
    #[serde(flatten)]
    pub meta: ProfileMeta,
    pub preferred: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// In-memory simulation. Never creates a real tunnel.
    Mock,
    NetworkManager,
    WindowsService,
    MacosDaemon,
    /// No usable backend detected.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
pub enum Support {
    Supported,
    Unsupported(String),
}

impl Support {
    pub fn is_supported(&self) -> bool {
        matches!(self, Support::Supported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend: BackendKind,
    pub simulated: bool,
    pub backend_version: Option<String>,
    pub available: Support,
    pub kill_switch: Support,
    pub dns_protection: Support,
    pub ipv6_protection: Support,
    pub exact_handshake_time: Support,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Activating,
    Up,
    Deactivating,
    Down,
}

/// Raw tunnel status reported by a backend for the active managed tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelStatus {
    pub id: ProfileId,
    pub link: LinkState,
    pub interface: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Unix ms of the most recent handshake, when the backend can read it.
    pub last_handshake_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerState {
    /// Tunnel is up but nothing proves the server answered yet.
    Pending,
    /// A handshake completed or authenticated data arrived from the peer.
    Confirmed,
    /// Data was sent for a while with no reply: likely wrong key/endpoint or blocked UDP.
    NoResponse,
}

/// Connection lifecycle. `Connected` means the backend reports the link up;
/// peer confirmation is tracked separately in [`PeerState`], and internet
/// reachability is a separate diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Preparing {
        profile: ProfileId,
    },
    Connecting {
        profile: ProfileId,
    },
    Connected {
        profile: ProfileId,
        since_ms: u64,
        peer: PeerState,
    },
    Reconnecting {
        profile: ProfileId,
        attempt: u32,
        next_retry_ms: u64,
    },
    Disconnecting {
        profile: ProfileId,
    },
    /// A tunnel may still be active; changing/deleting it is unsafe until reconciled.
    Uncertain {
        profile: Option<ProfileId>,
        reason: String,
    },
    Failed {
        profile: Option<ProfileId>,
        reason: String,
    },
}

impl ConnectionState {
    pub fn profile(&self) -> Option<ProfileId> {
        use ConnectionState::*;
        match self {
            Disconnected => None,
            Failed { profile, .. } | Uncertain { profile, .. } => *profile,
            Preparing { profile }
            | Connecting { profile }
            | Connected { profile, .. }
            | Reconnecting { profile, .. }
            | Disconnecting { profile } => Some(*profile),
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(
            self,
            ConnectionState::Disconnected | ConnectionState::Failed { .. }
        )
    }
}

/// State of one protection feature as actually established, never as merely requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
pub enum Feature {
    Off,
    Unsupported(String),
    /// Configured through the backend but not independently verified.
    Configured(String),
    /// Verified by an active check.
    Verified(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Protection {
    pub kill_switch: Feature,
    pub dns: Feature,
    pub ipv6: Feature,
}

impl Protection {
    pub fn off() -> Self {
        Self {
            kill_switch: Feature::Off,
            dns: Feature::Off,
            ipv6: Feature::Off,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_rate_bps: f64,
    pub tx_rate_bps: f64,
    pub last_handshake_ms: Option<u64>,
    pub last_response_ms: Option<u64>,
    pub interface: Option<String>,
    pub sampled_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub health: Vec<crate::registry::ServerHealth>,
    pub state: ConnectionState,
    pub protection: Protection,
    /// Whether the far end answered a lookup through the tunnel. `Off` means
    /// not tested. Never inferred from the link or peer state.
    pub reachability: Feature,
    /// Egress IPv4 observed via the tunnel DNS whoami. `Off` = not tested;
    /// `Verified` detail is the address itself.
    pub public_ip: Feature,
    pub stats: Option<Stats>,
    pub profile: Option<ProfileView>,
    pub simulated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub at_ms: u64,
    pub level: Level,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    /// Closing the window hides it to the tray instead of quitting the UI.
    pub close_to_tray: bool,
    pub launch_on_login: bool,
    pub connect_on_launch: bool,
    pub preferred_profile: Option<ProfileId>,
    pub favorite_countries: Vec<String>,
    pub quick_connect_country: Option<String>,
    pub auto_reconnect: bool,
    pub reconnect_max_attempts: u32,
    /// Requested by the user. Connecting fails if it cannot be established.
    pub kill_switch: bool,
    /// While the kill switch blocks, still allow private and link-local
    /// ranges (printers, NAS, router). Off by default.
    pub kill_switch_allow_lan: bool,
    /// Destinations (IP or CIDR) that skip the VPN and use the normal
    /// connection. Applies from the next connection. Empty by default.
    pub bypass: Vec<String>,
    pub notifications: bool,
    /// User-controllable DNS checks through the configured tunnel resolver.
    pub connectivity_check: bool,
    pub check_updates: bool,
    /// First-run setup finished or skipped.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            close_to_tray: true,
            launch_on_login: false,
            connect_on_launch: false,
            preferred_profile: None,
            favorite_countries: vec![],
            quick_connect_country: None,
            auto_reconnect: true,
            reconnect_max_attempts: 8,
            kill_switch: false,
            kill_switch_allow_lan: false,
            bypass: vec![],
            notifications: true,
            connectivity_check: true,
            check_updates: false,
            onboarded: false,
        }
    }
}

/// Events pushed to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum EngineEvent {
    Snapshot(Box<Snapshot>),
    Log(LogEntry),
    ProfilesChanged,
    SettingsChanged(Settings),
}

/// Something the operating system told the backend that the engine must react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    /// The computer woke from sleep or hibernation.
    Resumed,
    /// The way to the server may have changed (another Wi-Fi network, cable
    /// plugged in, primary connection switched).
    NetworkChanged,
}

/// Most bypass entries a profile of settings may hold.
pub const MAX_BYPASS: usize = 64;

/// Parse one bypass entry: an IP address or CIDR range, normalised to its
/// network. `/0` is refused because it would route everything around the VPN.
pub fn parse_bypass(entry: &str) -> crate::error::Result<ipnet::IpNet> {
    let t = entry.trim();
    let net = if let Ok(n) = t.parse::<ipnet::IpNet>() {
        n
    } else if let Ok(ip) = t.parse::<std::net::IpAddr>() {
        ipnet::IpNet::from(ip)
    } else {
        return Err(crate::error::CoreError::invalid(format!(
            "\"{t}\" is not an IP address or range such as 192.168.1.0/24"
        )));
    };
    if net.prefix_len() == 0 {
        return Err(crate::error::CoreError::invalid(
            "a range that covers every address would turn the VPN off",
        ));
    }
    Ok(net.trunc())
}

#[cfg(test)]
mod bypass_tests {
    use super::parse_bypass;

    #[test]
    fn accepts_addresses_and_ranges() {
        assert_eq!(parse_bypass("10.0.0.5").unwrap().to_string(), "10.0.0.5/32");
        assert_eq!(
            parse_bypass(" 192.168.1.7/24 ").unwrap().to_string(),
            "192.168.1.0/24"
        );
        assert_eq!(parse_bypass("fd00::1").unwrap().to_string(), "fd00::1/128");
    }

    #[test]
    fn rejects_garbage_and_everything() {
        for bad in [
            "",
            "hello",
            "1.2.3.4/33",
            "0.0.0.0/0",
            "::/0",
            "1.2.3.4 all",
        ] {
            assert!(parse_bypass(bad).is_err(), "{bad}");
        }
    }
}
