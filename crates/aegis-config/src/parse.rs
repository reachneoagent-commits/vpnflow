use std::net::IpAddr;

use ipnet::IpNet;

use crate::{ConfigError, Endpoint, Host, Interface, Issue, Peer, PublicKey, SecretKey, WgConfig};

/// Imported files larger than this are rejected before parsing.
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_PEERS: usize = 256;

const SCRIPT_MSG: &str = "runs shell commands as root when the tunnel starts or stops. \
Aegis VPN never executes scripts from profiles. Remove this line to import the profile; \
if you rely on it, keep using wg-quick for this tunnel.";

#[derive(Default)]
struct RawInterface {
    line: usize,
    private_key: Option<SecretKey>,
    addresses: Vec<IpNet>,
    dns: Vec<IpAddr>,
    dns_search: Vec<String>,
    mtu: Option<u16>,
    listen_port: Option<u16>,
}

struct RawPeer {
    line: usize,
    public_key: Option<PublicKey>,
    preshared_key: Option<SecretKey>,
    endpoint: Option<Endpoint>,
    allowed_ips: Vec<IpNet>,
    persistent_keepalive: Option<u16>,
}

enum Section {
    None,
    Interface,
    Peer,
}

/// Parse and validate `wg-quick` text. Returns every error found, not just the first.
pub fn parse(text: &str) -> Result<WgConfig, ConfigError> {
    parse_with(text, false).map(|(cfg, _)| cfg)
}

/// Like [`parse`], for files the user imports. A file with only IPv4 tunnel
/// addresses that still routes `::/0` (or lists IPv6 DNS) is imported as
/// IPv4-only instead of rejected; the IPv6 entries are dropped, and the
/// platform backends then block IPv6 so nothing leaks. The flag says whether
/// anything was dropped.
pub fn parse_import(text: &str) -> Result<(WgConfig, bool), ConfigError> {
    parse_with(text, true)
}

fn parse_with(text: &str, ipv4_only_fix: bool) -> Result<(WgConfig, bool), ConfigError> {
    let mut issues = Vec::new();
    if text.len() > MAX_CONFIG_BYTES {
        issues.push(Issue::error(
            None,
            None,
            format!("file is larger than {} KiB", MAX_CONFIG_BYTES / 1024),
        ));
        return Err(ConfigError { issues });
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let mut iface: Option<RawInterface> = None;
    let mut peers: Vec<RawPeer> = Vec::new();
    let mut section = Section::None;
    let mut seen: Vec<String> = Vec::new(); // scalar keys seen in current section

    for (idx, raw_line) in text.lines().enumerate() {
        let ln = idx + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            seen.clear();
            match line.to_ascii_lowercase().as_str() {
                "[interface]" => {
                    if iface.is_some() {
                        issues.push(Issue::error(
                            Some(ln),
                            None,
                            "duplicate [Interface] section",
                        ));
                    } else {
                        iface = Some(RawInterface {
                            line: ln,
                            ..Default::default()
                        });
                    }
                    section = Section::Interface;
                }
                "[peer]" => {
                    if peers.len() >= MAX_PEERS {
                        issues.push(Issue::error(
                            Some(ln),
                            None,
                            format!("more than {MAX_PEERS} peers"),
                        ));
                        break;
                    }
                    peers.push(RawPeer {
                        line: ln,
                        public_key: None,
                        preshared_key: None,
                        endpoint: None,
                        allowed_ips: Vec::new(),
                        persistent_keepalive: None,
                    });
                    section = Section::Peer;
                }
                _ => {
                    issues.push(Issue::error(
                        Some(ln),
                        None,
                        "unknown section (expected [Interface] or [Peer])",
                    ));
                    section = Section::None;
                }
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            issues.push(Issue::error(Some(ln), None, "expected `Key = Value`"));
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim();
        let display_key = printable_key(k.trim());

        let is_list = matches!(key.as_str(), "address" | "dns" | "allowedips");
        if !is_list {
            if seen.contains(&key) {
                issues.push(Issue::error(
                    Some(ln),
                    Some(&display_key),
                    "set more than once in this section",
                ));
                continue;
            }
            seen.push(key.clone());
        }

        let mut err = |msg: &str| issues.push(Issue::error(Some(ln), Some(&display_key), msg));
        match section {
            Section::None => err("appears outside an [Interface] or [Peer] section"),
            Section::Interface => {
                let i = iface.as_mut().expect("section implies interface exists");
                match key.as_str() {
                    "privatekey" => match val.parse() {
                        Ok(k) => i.private_key = Some(k),
                        Err(_) => err("is not a valid WireGuard key"),
                    },
                    "address" => for_each_item(val, |n, item| match parse_address(item) {
                        Some(a) => i.addresses.push(a),
                        None => err(&format!(
                            "entry {n} is not a valid IP address with optional /prefix"
                        )),
                    }),
                    "dns" => for_each_item(val, |n, item| {
                        if let Ok(ip) = item.parse::<IpAddr>() {
                            i.dns.push(ip);
                        } else if is_hostname(item) {
                            i.dns_search.push(item.to_ascii_lowercase());
                        } else {
                            err(&format!(
                                "entry {n} is neither an IP address nor a search domain"
                            ));
                        }
                    }),
                    "mtu" => match val.parse::<u16>() {
                        Ok(m) => i.mtu = Some(m),
                        Err(_) => err("must be a number"),
                    },
                    "listenport" => match val.parse::<u16>() {
                        Ok(p) => i.listen_port = Some(p),
                        Err(_) => err("must be a port number (0-65535)"),
                    },
                    "preup" | "postup" | "predown" | "postdown" => err(SCRIPT_MSG),
                    "saveconfig" => err(
                        "lets wg-quick overwrite this file at runtime; not supported. Remove this line.",
                    ),
                    "table" => err(
                        "custom routing tables are not supported; importing without it would change routing. Remove it only if default routing is acceptable.",
                    ),
                    "fwmark" => err(
                        "custom firewall marks are not supported; Aegis manages marks itself. Remove this line.",
                    ),
                    _ => err("unknown or unsupported key in [Interface]"),
                }
            }
            Section::Peer => {
                let p = peers.last_mut().expect("section implies peer exists");
                match key.as_str() {
                    "publickey" => match val.parse() {
                        Ok(k) => p.public_key = Some(k),
                        Err(_) => err("is not a valid WireGuard key"),
                    },
                    "presharedkey" => match val.parse() {
                        Ok(k) => p.preshared_key = Some(k),
                        Err(_) => err("is not a valid WireGuard key"),
                    },
                    "endpoint" => match parse_endpoint(val) {
                        Ok(e) => p.endpoint = Some(e),
                        Err(m) => err(m),
                    },
                    "allowedips" => for_each_item(val, |n, item| match item.parse::<IpNet>() {
                        Ok(net) => p.allowed_ips.push(net),
                        Err(_) => match item.parse::<IpAddr>() {
                            Ok(ip) => p.allowed_ips.push(IpNet::from(ip)),
                            Err(_) => err(&format!("entry {n} is not a valid IP network (CIDR)")),
                        },
                    }),
                    "persistentkeepalive" => {
                        if val.eq_ignore_ascii_case("off") {
                            p.persistent_keepalive = None;
                        } else {
                            match val.parse::<u16>() {
                                Ok(0) => p.persistent_keepalive = None,
                                Ok(s) => p.persistent_keepalive = Some(s),
                                Err(_) => err("must be `off` or a number of seconds (1-65535)"),
                            }
                        }
                    }
                    _ => err("unknown or unsupported key in [Peer]"),
                }
            }
        }
    }

    let Some(iface) = iface else {
        issues.push(Issue::error(None, None, "missing [Interface] section"));
        return Err(ConfigError { issues });
    };
    let Some(private_key) = iface.private_key else {
        issues.push(Issue::error(
            Some(iface.line),
            Some("PrivateKey"),
            "is required",
        ));
        return Err(ConfigError { issues });
    };
    let mut out_peers = Vec::with_capacity(peers.len());
    for p in peers {
        match p.public_key {
            Some(public_key) => out_peers.push(Peer {
                public_key,
                preshared_key: p.preshared_key,
                endpoint: p.endpoint,
                allowed_ips: p.allowed_ips,
                persistent_keepalive: p.persistent_keepalive,
            }),
            None => issues.push(Issue::error(
                Some(p.line),
                Some("PublicKey"),
                "is required in every [Peer]",
            )),
        }
    }
    let mut cfg = WgConfig {
        interface: Interface {
            private_key,
            addresses: iface.addresses,
            dns: iface.dns,
            dns_search: iface.dns_search,
            mtu: iface.mtu,
            listen_port: iface.listen_port,
        },
        peers: out_peers,
    };
    let mut fixed = false;
    let a = &cfg.interface.addresses;
    if ipv4_only_fix && !a.is_empty() && !a.iter().any(|n| n.addr().is_ipv6()) {
        let before =
            cfg.interface.dns.len() + cfg.peers.iter().map(|p| p.allowed_ips.len()).sum::<usize>();
        cfg.interface.dns.retain(|d| d.is_ipv4());
        for p in &mut cfg.peers {
            p.allowed_ips.retain(|n| n.addr().is_ipv4());
        }
        let after =
            cfg.interface.dns.len() + cfg.peers.iter().map(|p| p.allowed_ips.len()).sum::<usize>();
        fixed = after < before;
    }
    issues.extend(crate::validate(&cfg));
    if issues.is_empty() {
        Ok((cfg, fixed))
    } else {
        Err(ConfigError { issues })
    }
}

fn for_each_item(val: &str, mut f: impl FnMut(usize, &str)) {
    for (n, item) in val
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .enumerate()
    {
        f(n + 1, item);
    }
}

/// Key names are echoed in messages, so keep them short and printable.
fn printable_key(k: &str) -> String {
    let s: String = k
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect();
    if s.is_empty() { "(key)".into() } else { s }
}

fn parse_address(s: &str) -> Option<IpNet> {
    // Interface addresses keep host bits (10.0.0.2/24), so parse via IpNet
    // which preserves them, or a bare IP as a host route.
    s.parse::<IpNet>()
        .ok()
        .or_else(|| s.parse::<IpAddr>().ok().map(IpNet::from))
}

pub(crate) fn is_hostname(s: &str) -> bool {
    let s = s.strip_suffix('.').unwrap_or(s);
    !s.is_empty()
        && s.len() <= 253
        && s.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

pub(crate) fn parse_endpoint(s: &str) -> Result<Endpoint, &'static str> {
    const BAD_PORT: &str = "port must be 1-65535";
    let s = s.trim();
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (h, p) = rest
            .split_once("]:")
            .ok_or("IPv6 endpoints must look like [2001:db8::1]:51820")?;
        let ip: std::net::Ipv6Addr = h.parse().map_err(|_| "invalid IPv6 address in brackets")?;
        (Host::Ip(IpAddr::V6(ip)), p)
    } else {
        let (h, p) = s.rsplit_once(':').ok_or("must be host:port")?;
        if h.contains(':') {
            return Err("IPv6 endpoints must be in brackets, e.g. [2001:db8::1]:51820");
        }
        let host = match h.parse::<IpAddr>() {
            Ok(ip) => Host::Ip(ip),
            Err(_) if is_hostname(h) => Host::Name(h.to_ascii_lowercase()),
            Err(_) => return Err("host is not a valid IP address or DNS name"),
        };
        (host, p)
    };
    match port.parse::<u16>() {
        Ok(0) | Err(_) => Err(BAD_PORT),
        Ok(port) => Ok(Endpoint { host, port }),
    }
}
