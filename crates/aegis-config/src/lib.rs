//! WireGuard (`wg-quick` style) configuration parsing, validation and
//! serialization.
//!
//! Security rules enforced here:
//! - Executable directives (`PreUp`, `PostUp`, `PreDown`, `PostDown`) and
//!   `SaveConfig` are rejected, never stored, never run.
//! - Directives that change routing semantics we cannot honour (`Table`,
//!   `FwMark`) are rejected rather than silently dropped.
//! - Unknown keys are rejected rather than silently dropped.
//! - Error messages never echo values, so parsing a wrong file cannot leak
//!   its contents through the UI.

mod key;
mod parse;
mod validate;

use std::fmt;
use std::net::IpAddr;

use ipnet::IpNet;
use zeroize::Zeroizing;

pub use key::{InvalidKey, KEY_LEN, PublicKey, SecretKey};
pub use parse::{MAX_CONFIG_BYTES, parse, parse_import};
pub use validate::{validate, warnings};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgConfig {
    pub interface: Interface,
    pub peers: Vec<Peer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    pub private_key: SecretKey,
    pub addresses: Vec<IpNet>,
    pub dns: Vec<IpAddr>,
    pub dns_search: Vec<String>,
    pub mtu: Option<u16>,
    pub listen_port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub public_key: PublicKey,
    pub preshared_key: Option<SecretKey>,
    pub endpoint: Option<Endpoint>,
    pub allowed_ips: Vec<IpNet>,
    /// Seconds; `None` means off.
    pub persistent_keepalive: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Host {
    Ip(IpAddr),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Endpoint {
    pub host: Host,
    pub port: u16,
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.host {
            Host::Ip(IpAddr::V6(ip)) => write!(f, "[{ip}]:{}", self.port),
            Host::Ip(ip) => write!(f, "{ip}:{}", self.port),
            Host::Name(n) => write!(f, "{n}:{}", self.port),
        }
    }
}

impl std::str::FromStr for Endpoint {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse::parse_endpoint(s)
    }
}

/// How a profile routes traffic. Derived, never altered by the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// All IPv4 and IPv6 traffic goes through the tunnel.
    Full,
    /// All IPv4 goes through the tunnel; IPv6 does not (possible IPv6 leak).
    FullIpv4Only,
    /// All IPv6 goes through the tunnel; IPv4 does not.
    FullIpv6Only,
    /// Only specific networks go through the tunnel.
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Issue {
    pub severity: Severity,
    /// 1-based source line, when the issue came from parsing text.
    pub line: Option<usize>,
    pub field: Option<String>,
    pub message: String,
}

impl Issue {
    pub(crate) fn error(
        line: Option<usize>,
        field: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            line,
            field: field.map(str::to_owned),
            message: message.into(),
        }
    }
    pub(crate) fn warning(field: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            line: None,
            field: field.map(str::to_owned),
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(l) = self.line {
            write!(f, "line {l}: ")?;
        }
        if let Some(field) = &self.field {
            write!(f, "{field}: ")?;
        }
        f.write_str(&self.message)
    }
}

/// One or more blocking problems with a configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub struct ConfigError {
    pub issues: Vec<Issue>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.issues.iter().map(ToString::to_string).collect();
        write!(f, "invalid WireGuard configuration: {}", parts.join("; "))
    }
}

fn covers_default(nets: &[&IpNet], v6: bool) -> bool {
    // Covering both halves handles `/0` and the common `0.0.0.0/1 + 128.0.0.0/1` trick.
    let halves: [IpNet; 2] = if v6 {
        [
            "::/1".parse().expect("static"),
            "8000::/1".parse().expect("static"),
        ]
    } else {
        [
            "0.0.0.0/1".parse().expect("static"),
            "128.0.0.0/1".parse().expect("static"),
        ]
    };
    halves.iter().all(|h| nets.iter().any(|n| n.contains(h)))
}

impl WgConfig {
    pub fn route_kind(&self) -> RouteKind {
        let all: Vec<&IpNet> = self
            .peers
            .iter()
            .flat_map(|p| p.allowed_ips.iter())
            .collect();
        match (covers_default(&all, false), covers_default(&all, true)) {
            (true, true) => RouteKind::Full,
            (true, false) => RouteKind::FullIpv4Only,
            (false, true) => RouteKind::FullIpv6Only,
            (false, false) => RouteKind::Split,
        }
    }

    /// Serialize to `wg-quick` text, INCLUDING SECRETS. Only call for an
    /// explicit, user-confirmed export or a privileged backend write.
    pub fn to_wg_quick(&self) -> Zeroizing<String> {
        use std::fmt::Write as _;
        let join = |v: &[IpNet]| {
            v.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut s = Zeroizing::new(String::new());
        let i = &self.interface;
        let _ = writeln!(s, "[Interface]");
        let _ = writeln!(s, "PrivateKey = {}", i.private_key.expose_base64().as_str());
        let _ = writeln!(s, "Address = {}", join(&i.addresses));
        let dns: Vec<String> = i
            .dns
            .iter()
            .map(ToString::to_string)
            .chain(i.dns_search.iter().cloned())
            .collect();
        if !dns.is_empty() {
            let _ = writeln!(s, "DNS = {}", dns.join(", "));
        }
        if let Some(m) = i.mtu {
            let _ = writeln!(s, "MTU = {m}");
        }
        if let Some(p) = i.listen_port {
            let _ = writeln!(s, "ListenPort = {p}");
        }
        for p in &self.peers {
            let _ = writeln!(s, "\n[Peer]");
            let _ = writeln!(s, "PublicKey = {}", p.public_key);
            if let Some(psk) = &p.preshared_key {
                let _ = writeln!(s, "PresharedKey = {}", psk.expose_base64().as_str());
            }
            if let Some(e) = &p.endpoint {
                let _ = writeln!(s, "Endpoint = {e}");
            }
            let _ = writeln!(s, "AllowedIPs = {}", join(&p.allowed_ips));
            if let Some(k) = p.persistent_keepalive {
                let _ = writeln!(s, "PersistentKeepalive = {k}");
            }
        }
        s
    }
}

#[cfg(test)]
mod tests;
