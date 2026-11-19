//! Pure builders and parsers for the `ifconfig`, `route` and `networksetup`
//! calls. Arguments are built from validated types (IpNet, IpAddr, `utunN`),
//! never from raw strings, and commands are run without a shell.

use std::net::IpAddr;

use aegis_macos::pf::valid_tunnel_interface;
use ipnet::IpNet;

pub fn ifconfig_address(iface: &str, addr: &IpNet) -> Vec<String> {
    debug_assert!(valid_tunnel_interface(iface));
    match addr {
        IpNet::V4(n) => vec![
            iface.into(),
            "inet".into(),
            format!("{}/{}", n.addr(), n.prefix_len()),
            n.addr().to_string(),
            "alias".into(),
        ],
        IpNet::V6(n) => vec![
            iface.into(),
            "inet6".into(),
            n.addr().to_string(),
            "prefixlen".into(),
            n.prefix_len().to_string(),
            "alias".into(),
        ],
    }
}

/// Default routes are split in two halves so they win over the real default
/// route without replacing it.
pub fn expand_route(net: &IpNet) -> Vec<IpNet> {
    let parse = |s: &str| s.parse::<IpNet>().expect("static net");
    match net {
        IpNet::V4(n) if n.prefix_len() == 0 => vec![parse("0.0.0.0/1"), parse("128.0.0.0/1")],
        IpNet::V6(n) if n.prefix_len() == 0 => vec![parse("::/1"), parse("8000::/1")],
        other => vec![*other],
    }
}

pub fn route_add_interface(net: &IpNet, iface: &str) -> Vec<String> {
    let fam = if net.addr().is_ipv4() {
        "-inet"
    } else {
        "-inet6"
    };
    vec![
        "-q".into(),
        "-n".into(),
        "add".into(),
        fam.into(),
        net.to_string(),
        "-interface".into(),
        iface.into(),
    ]
}

/// Host route for a WireGuard endpoint via the pre-VPN gateway, so the
/// encrypted packets do not loop into the tunnel.
pub fn route_add_endpoint(ip: IpAddr, gw: &Gateway) -> Vec<String> {
    let fam = if ip.is_ipv4() { "-inet" } else { "-inet6" };
    let mut v = vec![
        "-q".into(),
        "-n".into(),
        "add".into(),
        fam.into(),
        "-host".into(),
        ip.to_string(),
    ];
    match gw {
        Gateway::Via(g) => v.push(g.to_string()),
        Gateway::Interface(i) => {
            v.push("-interface".into());
            v.push(i.clone());
        }
    }
    v
}

/// Route a whole network via the pre-VPN gateway so it skips the tunnel.
pub fn route_add_bypass(net: &IpNet, gw: &Gateway) -> Vec<String> {
    let fam = if net.addr().is_ipv4() {
        "-inet"
    } else {
        "-inet6"
    };
    let mut v = vec![
        "-q".into(),
        "-n".into(),
        "add".into(),
        fam.into(),
        "-net".into(),
        net.to_string(),
    ];
    match gw {
        Gateway::Via(g) => v.push(g.to_string()),
        Gateway::Interface(i) => {
            v.push("-interface".into());
            v.push(i.clone());
        }
    }
    v
}

pub fn route_delete_net(net: &IpNet) -> Vec<String> {
    let fam = if net.addr().is_ipv4() {
        "-inet"
    } else {
        "-inet6"
    };
    vec![
        "-q".into(),
        "-n".into(),
        "delete".into(),
        fam.into(),
        "-net".into(),
        net.to_string(),
    ]
}

pub fn route_delete_host(ip: IpAddr) -> Vec<String> {
    let fam = if ip.is_ipv4() { "-inet" } else { "-inet6" };
    vec![
        "-q".into(),
        "-n".into(),
        "delete".into(),
        fam.into(),
        "-host".into(),
        ip.to_string(),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gateway {
    Via(IpAddr),
    Interface(String),
}

/// Parse `route -n get default` (or `-inet6`).
pub fn parse_default_gateway(out: &str) -> Option<Gateway> {
    let mut gw = None;
    let mut iface = None;
    for line in out.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("gateway:") {
            gw = v
                .trim()
                .split('%')
                .next()
                .and_then(|s| s.parse::<IpAddr>().ok());
        } else if let Some(v) = line.strip_prefix("interface:") {
            iface = Some(v.trim().to_owned());
        }
    }
    match (gw, iface) {
        (Some(g), _) => Some(Gateway::Via(g)),
        (None, Some(i)) if !i.starts_with("utun") => Some(Gateway::Interface(i)),
        _ => None,
    }
}

/// Service names from `networksetup -listallnetworkservices`; skips the
/// header line and disabled (`*`-prefixed) services.
pub fn parse_services(out: &str) -> Vec<String> {
    out.lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('*'))
        .map(str::to_owned)
        .collect()
}

/// `networksetup -getdnsservers`: addresses, or empty when none are set.
pub fn parse_dns_servers(out: &str) -> Vec<IpAddr> {
    out.lines().filter_map(|l| l.trim().parse().ok()).collect()
}

/// `networksetup -setdnsservers <service> <ip...|Empty>`.
pub fn set_dns_args(service: &str, servers: &[IpAddr]) -> Vec<String> {
    let mut v = vec!["-setdnsservers".into(), service.into()];
    if servers.is_empty() {
        v.push("Empty".into());
    } else {
        v.extend(servers.iter().map(ToString::to_string));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(s: &str) -> IpNet {
        s.parse().unwrap()
    }

    #[test]
    fn default_routes_are_split() {
        assert_eq!(
            expand_route(&net("0.0.0.0/0")),
            vec![net("0.0.0.0/1"), net("128.0.0.0/1")]
        );
        assert_eq!(
            expand_route(&net("::/0")),
            vec![net("::/1"), net("8000::/1")]
        );
        assert_eq!(expand_route(&net("10.0.0.0/8")), vec![net("10.0.0.0/8")]);
    }

    #[test]
    fn address_and_route_args() {
        assert_eq!(
            ifconfig_address("utun4", &net("10.2.0.2/32")),
            ["utun4", "inet", "10.2.0.2/32", "10.2.0.2", "alias"]
        );
        assert_eq!(
            route_add_interface(&net("0.0.0.0/1"), "utun4").join(" "),
            "-q -n add -inet 0.0.0.0/1 -interface utun4"
        );
    }

    #[test]
    fn parses_gateway() {
        let out = "   route to: default\ndestination: default\n       mask: default\n    gateway: 192.168.1.1\n  interface: en0\n";
        assert_eq!(
            parse_default_gateway(out),
            Some(Gateway::Via("192.168.1.1".parse().unwrap()))
        );
        let on_link = "    interface: en0\n";
        assert_eq!(
            parse_default_gateway(on_link),
            Some(Gateway::Interface("en0".into()))
        );
        assert_eq!(parse_default_gateway("    interface: utun3\n"), None);
    }

    #[test]
    fn parses_dns_listings() {
        let out = "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\n*Thunderbolt Bridge\nUSB 10/100/1000 LAN\n";
        assert_eq!(parse_services(out), ["Wi-Fi", "USB 10/100/1000 LAN"]);
        assert!(parse_dns_servers("There aren't any DNS Servers set on Wi-Fi.\n").is_empty());
        assert_eq!(parse_dns_servers("1.1.1.1\n9.9.9.9\n").len(), 2);
        assert_eq!(set_dns_args("Wi-Fi", &[]).last().unwrap(), "Empty");
    }
}
