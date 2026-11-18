//! Kill-switch ruleset for macOS `pf`, loaded into the `vpnflow` anchor by the
//! root daemon. Portable and pure so the rules are unit-tested on any host.
//!
//! Policy (mirrors `Backend::apply_kill_switch`): block everything, then pass
//! loopback, the tunnel interface, UDP to the WireGuard endpoints, DHCP, IPv6
//! neighbour discovery and, when `allow_lan`, private/link-local ranges.
//! Every input is validated; nothing user-controlled is interpolated raw.

use std::net::SocketAddr;

pub const ANCHOR: &str = "vpnflow";

const LAN_V4: &str = "{ 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16 }";
const LAN_V6: &str = "{ fe80::/10, fc00::/7 }";

#[derive(Debug, PartialEq, Eq)]
pub enum PfError {
    BadInterface,
    NoEndpoints,
}

/// `utun` followed by digits only, at most `utun9999`.
pub fn valid_tunnel_interface(name: &str) -> bool {
    name.strip_prefix("utun")
        .is_some_and(|n| !n.is_empty() && n.len() <= 4 && n.bytes().all(|b| b.is_ascii_digit()))
}

pub fn ruleset(
    tunnel_if: &str,
    endpoints: &[SocketAddr],
    allow_lan: bool,
) -> Result<String, PfError> {
    if !valid_tunnel_interface(tunnel_if) {
        return Err(PfError::BadInterface);
    }
    if endpoints.is_empty() {
        return Err(PfError::NoEndpoints);
    }
    let mut r = String::new();
    r.push_str("set skip on lo0\n");
    r.push_str("block drop all\n");
    r.push_str(&format!("pass quick on {tunnel_if} all\n"));
    for ep in endpoints {
        // SocketAddr's Display is `ip:port` / `[ip6]:port`; pf wants them split.
        let fam = if ep.is_ipv4() { "inet" } else { "inet6" };
        r.push_str(&format!(
            "pass out quick {fam} proto udp to {} port {}\n",
            ep.ip(),
            ep.port()
        ));
    }
    r.push_str("pass out quick inet proto udp from any port 68 to any port 67\n");
    r.push_str("pass in quick inet proto udp from any port 67 to any port 68\n");
    r.push_str(
        "pass quick inet6 proto icmp6 all icmp6-type { routersol, routeradv, neighbrsol, neighbradv }\n",
    );
    if allow_lan {
        r.push_str(&format!("pass quick inet to {LAN_V4}\n"));
        r.push_str(&format!("pass quick inet from {LAN_V4}\n"));
        r.push_str(&format!("pass quick inet6 to {LAN_V6}\n"));
        r.push_str(&format!("pass quick inet6 from {LAN_V6}\n"));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn interface_names_are_strict() {
        assert!(valid_tunnel_interface("utun4"));
        assert!(!valid_tunnel_interface("utun"));
        assert!(!valid_tunnel_interface("en0"));
        assert!(!valid_tunnel_interface("utun4 all\npass out"));
        assert!(!valid_tunnel_interface("utun12345"));
    }

    #[test]
    fn blocks_by_default_and_permits_endpoint_and_tunnel() {
        let r = ruleset(
            "utun4",
            &[ep("203.0.113.7:51820"), ep("[2001:db8::1]:51820")],
            false,
        )
        .unwrap();
        assert!(r.contains("block drop all\n"));
        assert!(r.contains("pass quick on utun4 all\n"));
        assert!(r.contains("pass out quick inet proto udp to 203.0.113.7 port 51820\n"));
        assert!(r.contains("pass out quick inet6 proto udp to 2001:db8::1 port 51820\n"));
        assert!(!r.contains("192.168"));
    }

    #[test]
    fn lan_rules_only_when_allowed() {
        let on = ruleset("utun4", &[ep("203.0.113.7:51820")], true).unwrap();
        assert!(on.contains("192.168.0.0/16"));
        assert!(on.contains("fe80::/10"));
    }

    #[test]
    fn refuses_bad_input() {
        assert_eq!(
            ruleset("en0", &[ep("1.2.3.4:1")], false),
            Err(PfError::BadInterface)
        );
        assert_eq!(ruleset("utun1", &[], false), Err(PfError::NoEndpoints));
    }
}
