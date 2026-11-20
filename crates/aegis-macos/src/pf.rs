//! Kill-switch ruleset for macOS `pf`, loaded into the `vpnflow` anchor by the
//! root daemon. Portable and pure so the rules are unit-tested on any host.
//!
//! Policy (mirrors `Backend::apply_kill_switch`): block everything, then pass
//! loopback, the tunnel interface, UDP to the WireGuard endpoints, DHCP, IPv6
//! neighbour discovery and, when `allow_lan`, private/link-local ranges.
//! DNS (ports 53 and 853) outside the tunnel is blocked even for those ranges.
//! Every input is validated; nothing user-controlled is interpolated raw.

use std::net::SocketAddr;

use ipnet::IpNet;

/// Sub-anchor of `com.apple/*`, which macOS's stock /etc/pf.conf already
/// evaluates, so no edit of the system ruleset is needed.
pub const ANCHOR: &str = "com.apple/vpnflow";

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
    tunnel_if: Option<&str>,
    endpoints: &[SocketAddr],
    allow_lan: bool,
) -> Result<String, PfError> {
    ruleset_with_bypass(tunnel_if, endpoints, allow_lan, &[])
}

/// Like [`ruleset`], also passing traffic to and from the `bypass` networks
/// (which the daemon routes around the tunnel).
pub fn ruleset_with_bypass(
    tunnel_if: Option<&str>,
    endpoints: &[SocketAddr],
    allow_lan: bool,
    bypass: &[IpNet],
) -> Result<String, PfError> {
    if tunnel_if.is_some_and(|i| !valid_tunnel_interface(i)) {
        return Err(PfError::BadInterface);
    }
    if endpoints.is_empty() {
        return Err(PfError::NoEndpoints);
    }
    let mut r = String::new();
    // `set skip` is a global option that pf does not apply inside an anchor, so
    // loopback (localhost dev servers, local tools) is passed with explicit
    // rules, first, in both directions and for both IP families.
    r.push_str("pass quick on lo0 all\n");
    // AirDrop, AirPlay and Sidecar talk to nearby Apple devices over Apple's
    // direct radio links (awdl0, llw0). That traffic never reaches the internet.
    r.push_str("pass quick on awdl0 all\n");
    r.push_str("pass quick on llw0 all\n");
    if let Some(tunnel_if) = tunnel_if {
        r.push_str(&format!("pass quick on {tunnel_if} all\n"));
    }
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
    for net in bypass {
        let fam = if net.addr().is_ipv4() {
            "inet"
        } else {
            "inet6"
        };
        r.push_str(&format!("pass quick {fam} to {net}\n"));
        r.push_str(&format!("pass quick {fam} from {net}\n"));
    }
    // DNS (and DNS over TLS) may only travel inside the tunnel. Without this,
    // "allow local network" would let lookups reach the router, which forwards
    // them to the internet provider. Traffic inside the tunnel already passed
    // above; destinations the user chose to bypass are passed before this line.
    r.push_str("block drop quick proto { tcp, udp } from any to any port { 53, 853 }\n");
    if allow_lan {
        r.push_str(&format!("pass quick inet to {LAN_V4}\n"));
        r.push_str(&format!("pass quick inet from {LAN_V4}\n"));
        r.push_str(&format!("pass quick inet6 to {LAN_V6}\n"));
        r.push_str(&format!("pass quick inet6 from {LAN_V6}\n"));
    }
    // Last and quick: nothing evaluated after this anchor can re-allow traffic.
    r.push_str("block drop quick all\n");
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn loopback_is_passed_first_so_localhost_keeps_working() {
        let r = ruleset(Some("utun4"), &[ep("203.0.113.7:51820")], false).unwrap();
        assert!(r.starts_with("pass quick on lo0 all\n"));
        assert!(!r.contains("set skip"));
        let r = ruleset(None, &[ep("203.0.113.7:51820")], false).unwrap();
        assert!(r.starts_with("pass quick on lo0 all\n"));
    }

    #[test]
    fn airdrop_links_are_passed_before_the_final_block() {
        let r = ruleset(Some("utun4"), &[ep("203.0.113.7:51820")], false).unwrap();
        let awdl = r.find("pass quick on awdl0 all\n").unwrap();
        assert!(awdl < r.find("block drop quick all").unwrap());
        assert!(r.contains("pass quick on llw0 all\n"));
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
    fn dns_outside_the_tunnel_is_blocked_even_with_local_network_allowed() {
        let nets: Vec<IpNet> = vec!["192.168.1.5/32".parse().unwrap()];
        let r =
            ruleset_with_bypass(Some("utun4"), &[ep("203.0.113.7:51820")], true, &nets).unwrap();
        let pos = |needle: &str| r.find(needle).unwrap_or_else(|| panic!("missing {needle}"));
        let dns = pos("block drop quick proto { tcp, udp } from any to any port { 53, 853 }\n");
        assert!(
            pos("pass quick on utun4 all\n") < dns,
            "tunnel DNS must pass"
        );
        assert!(
            pos("pass quick inet to 192.168.1.5/32\n") < dns,
            "bypass choice wins"
        );
        assert!(
            dns < pos("pass quick inet to { 10.0.0.0/8"),
            "LAN must not re-allow DNS"
        );
        assert!(dns < pos("pass quick inet6 to { fe80::/10"));
        assert!(r.trim_end().ends_with("block drop quick all"));
    }

    #[test]
    fn bypass_networks_are_passed_before_the_final_block() {
        let nets: Vec<IpNet> = vec![
            "203.0.113.0/24".parse().unwrap(),
            "fd00::/8".parse().unwrap(),
        ];
        let r =
            ruleset_with_bypass(Some("utun4"), &[ep("203.0.113.7:51820")], false, &nets).unwrap();
        assert!(r.contains("pass quick inet to 203.0.113.0/24\n"));
        assert!(r.contains("pass quick inet from 203.0.113.0/24\n"));
        assert!(r.contains("pass quick inet6 to fd00::/8\n"));
        assert!(r.trim_end().ends_with("block drop quick all"));
        assert!(
            !ruleset(Some("utun4"), &[ep("203.0.113.7:51820")], false)
                .unwrap()
                .contains("203.0.113.0/24")
        );
    }

    #[test]
    fn blocks_by_default_and_permits_endpoint_and_tunnel() {
        let r = ruleset(
            Some("utun4"),
            &[ep("203.0.113.7:51820"), ep("[2001:db8::1]:51820")],
            false,
        )
        .unwrap();
        assert!(r.ends_with("block drop quick all\n"));
        assert!(r.contains("pass quick on utun4 all\n"));
        assert!(r.contains("pass out quick inet proto udp to 203.0.113.7 port 51820\n"));
        assert!(r.contains("pass out quick inet6 proto udp to 2001:db8::1 port 51820\n"));
        assert!(!r.contains("192.168"));
    }

    #[test]
    fn lan_rules_only_when_allowed() {
        let on = ruleset(Some("utun4"), &[ep("203.0.113.7:51820")], true).unwrap();
        assert!(on.contains("192.168.0.0/16"));
        assert!(on.contains("fe80::/10"));
    }

    #[test]
    fn without_a_tunnel_only_the_endpoint_is_reachable() {
        let r = ruleset(None, &[ep("203.0.113.7:51820")], false).unwrap();
        assert!(!r.contains("on utun"));
        assert!(r.contains("to 203.0.113.7 port 51820"));
    }

    #[test]
    fn refuses_bad_input() {
        assert_eq!(
            ruleset(Some("en0"), &[ep("1.2.3.4:1")], false),
            Err(PfError::BadInterface)
        );
        assert_eq!(
            ruleset(Some("utun1"), &[], false),
            Err(PfError::NoEndpoints)
        );
    }
}
