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
    AndroidVpn,
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

/// A network the user trusts, such as home Wi-Fi. `id` is the router's
/// hardware address (macOS hides Wi-Fi names from apps without a location
/// permission, the router address needs none and also covers Ethernet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedNetwork {
    pub id: String,
    pub name: String,
}

/// Minutes after midnight for an `HH:MM` string, or None if malformed.
pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Most trusted networks a user may keep.
pub const MAX_TRUSTED: usize = 20;

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
    /// When a server stops responding, try other servers in the same
    /// country automatically.
    pub auto_switch_server: bool,
    /// Measure download and upload speed once each connection is stable.
    pub auto_speed_test: bool,
    /// Requested by the user. Connecting fails if it cannot be established.
    pub kill_switch: bool,
    /// The user has set the kill switch themselves. Until then it follows
    /// the default: on wherever the system supports it.
    pub kill_switch_chosen: bool,
    /// While the kill switch blocks, still allow private and link-local
    /// ranges (printers, NAS, router). On by default.
    pub kill_switch_allow_lan: bool,
    /// While the kill switch blocks, still let the Tailscale app reach its
    /// relay (DERP) and sign-in servers, so a tailnet keeps working when no
    /// direct path exists. On by default; it does nothing without
    /// Tailscale installed. macOS only for now.
    pub kill_switch_allow_tailscale: bool,
    /// Destinations (IP or CIDR) that skip the VPN and use the normal
    /// connection. Applies from the next connection. Empty by default.
    pub bypass: Vec<String>,
    /// Websites (host names such as `example.com`) that skip the VPN while
    /// connected, for sites that block VPN users. Their addresses are looked
    /// up through the VPN and routed around it. Empty by default.
    pub bypass_sites: Vec<String>,
    /// Name servers to use inside the tunnel instead of the ones in the
    /// profile (for example an ad-blocking resolver). Empty keeps the
    /// profile's own. Applies from the next connection.
    pub dns_servers: Vec<String>,
    /// When on, the VPN switches itself off on a trusted network and on
    /// everywhere else. Acts only when the network changes.
    pub trusted_networks_enabled: bool,
    /// Networks the user marked as trusted, identified by the router.
    pub trusted_networks: Vec<TrustedNetwork>,
    /// Connect and disconnect at set times of day.
    pub schedule_enabled: bool,
    /// Local time to connect, `HH:MM`.
    pub schedule_connect: String,
    /// Local time to disconnect, `HH:MM`.
    pub schedule_disconnect: String,
    /// Days the schedule runs, 1 = Monday to 7 = Sunday.
    pub schedule_days: Vec<u8>,
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
            auto_switch_server: true,
            auto_speed_test: true,
            kill_switch: false,
            kill_switch_chosen: false,
            kill_switch_allow_lan: true,
            kill_switch_allow_tailscale: true,
            bypass: vec![],
            bypass_sites: vec![],
            dns_servers: vec![],
            trusted_networks_enabled: false,
            trusted_networks: vec![],
            schedule_enabled: false,
            schedule_connect: "09:00".into(),
            schedule_disconnect: "17:00".into(),
            schedule_days: vec![1, 2, 3, 4, 5],
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
// Settings is sent unboxed on purpose: events are rare and serialised for
// the UI, so the size difference costs nothing measurable.
#[allow(clippy::large_enum_variant)]
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

/// Most websites the "skip the VPN" site list may hold.
pub const MAX_BYPASS_SITES: usize = 32;

/// Parse one site entry into a lowercase host name. Accepts a pasted link
/// (`https://www.example.com/path`) and drops the scheme, path, port and a
/// leading `www.` (both the name and its `www.` form are skipped).
pub fn parse_bypass_site(entry: &str) -> crate::error::Result<String> {
    let t = entry.trim();
    let bad = || {
        crate::error::CoreError::invalid(format!(
            "\"{t}\" is not a website name such as example.com"
        ))
    };
    let mut h = t.to_ascii_lowercase();
    if let Some((_, rest)) = h.split_once("://") {
        h = rest.to_owned();
    }
    let h = h.split(['/', '?', '#']).next().unwrap_or_default();
    if h.contains('@') {
        return Err(bad());
    }
    let h = h
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_end_matches('.');
    let h = h.strip_prefix("www.").unwrap_or(h);
    let labels: Vec<&str> = h.split('.').collect();
    let ok = h.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        // A bare address belongs in the address list, not here.
        && !labels.last().is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()));
    if ok { Ok(h.to_owned()) } else { Err(bad()) }
}

/// Addresses a skipped website may not have: anything local or private would
/// let a site's DNS answer open a path to the home network or this computer.
pub fn public_site_addr(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_multicast()
                || v.is_broadcast()
                || (o[0] == 100 && (64..128).contains(&o[1])))
        }
        std::net::IpAddr::V6(v) => {
            let s = v.segments()[0];
            !(v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || (s & 0xfe00) == 0xfc00
                || (s & 0xffc0) == 0xfe80
                || v.to_ipv4_mapped().is_some())
        }
    }
}

/// Most custom DNS servers that may be set.
pub const MAX_DNS_SERVERS: usize = 4;

/// Parse a custom DNS server: a public IPv4 address. Private, loopback,
/// link-local and similar addresses are refused because the lookups travel
/// through the tunnel and could never reach them there.
pub fn parse_dns_server(entry: &str) -> crate::error::Result<std::net::Ipv4Addr> {
    let t = entry.trim();
    let bad = |why: &str| {
        crate::error::CoreError::invalid(format!("\"{t}\" cannot be used as a DNS server: {why}"))
    };
    let ip = t
        .parse::<std::net::Ipv4Addr>()
        .map_err(|_| bad("use an IPv4 address such as 94.140.14.14"))?;
    let o = ip.octets();
    let cgnat = o[0] == 100 && (64..128).contains(&o[1]);
    if ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        || cgnat
    {
        return Err(bad("it must be a public address reachable through the VPN"));
    }
    Ok(ip)
}

#[cfg(test)]
mod dns_tests {
    use super::parse_dns_server;

    #[test]
    fn accepts_public_ipv4_only() {
        assert_eq!(
            parse_dns_server(" 94.140.14.14 ").unwrap().to_string(),
            "94.140.14.14"
        );
        for bad in [
            "10.2.0.1",
            "192.168.1.1",
            "127.0.0.1",
            "0.0.0.0",
            "169.254.1.1",
            "100.64.0.1",
            "224.0.0.1",
            "2606:4700::1111",
            "dns.google",
            "",
        ] {
            assert!(parse_dns_server(bad).is_err(), "{bad}");
        }
    }
}

#[cfg(test)]
mod site_tests {
    use super::parse_bypass_site;

    #[test]
    fn normalises_pasted_links() {
        for (input, want) in [
            ("https://www.metrobyt-mobile.com/", "metrobyt-mobile.com"),
            (" Example.COM ", "example.com"),
            ("http://shop.example.co.uk:8443/a?b#c", "shop.example.co.uk"),
            ("example.com.", "example.com"),
            ("xn--bcher-kva.example", "xn--bcher-kva.example"),
        ] {
            assert_eq!(parse_bypass_site(input).unwrap(), want, "{input}");
        }
    }

    #[test]
    fn refuses_non_names() {
        for bad in [
            "",
            "localhost",
            "www.",
            "192.168.1.1",
            "a..b",
            "-a.com",
            "exa mple.com",
            "user@example.com",
            "https://user:pw@example.com/",
            "ex_ample.com",
            "bücher.example",
        ] {
            assert!(parse_bypass_site(bad).is_err(), "{bad}");
        }
    }
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

#[cfg(test)]
mod site_addr_tests {
    use super::public_site_addr;

    #[test]
    fn site_addresses_must_be_public() {
        for ok in ["198.51.100.7", "23.45.67.89", "2606:4700::1111"] {
            assert!(public_site_addr(&ok.parse().unwrap()), "{ok}");
        }
        for bad in [
            "192.168.20.4",
            "10.0.0.1",
            "172.16.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "100.100.100.100",
            "0.0.0.0",
            "224.0.0.1",
            "::1",
            "fd7a:115c:a1e0::1",
            "fe80::1",
            "::ffff:192.168.1.1",
        ] {
            assert!(!public_site_addr(&bad.parse().unwrap()), "{bad}");
        }
    }
}
