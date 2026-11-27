//! Kill-switch ruleset for macOS `pf`, loaded into the `vpnflow` anchor by the
//! root daemon. Portable and pure so the rules are unit-tested on any host.
//!
//! Policy (mirrors `Backend::apply_kill_switch`): block everything, then pass
//! loopback, the tunnel interface, UDP to the WireGuard endpoints, DHCP, IPv6
//! neighbour discovery, the Tailscale/TailFlow ranges and, when `allow_lan`,
//! private/link-local ranges.
//! DNS (ports 53 and 853) outside the tunnel is blocked even for those ranges.
//! Every input is validated; nothing user-controlled is interpolated raw.

use std::net::{IpAddr, SocketAddr};

use ipnet::IpNet;

/// Sub-anchor of `com.apple/*`, which macOS's stock /etc/pf.conf already
/// evaluates, so no edit of the system ruleset is needed.
pub const ANCHOR: &str = "com.apple/vpnflow";

const LAN_V4: &str = "{ 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16 }";
const LAN_V6: &str = "{ fe80::/10, fc00::/7 }";
/// Tailscale-style mesh networks (Tailscale, Headscale/TailFlow): device
/// addresses and MagicDNS (100.100.100.100) live in the CGNAT range, IPv6 in
/// Tailscale's ULA prefix. Traffic to the home network over a subnet route
/// leaves with a tailnet source address, so it is covered too. Only these
/// destinations skip the block; the public internet stays blocked.
pub const TAILNET_V4: &str = "100.64.0.0/10";
pub const TAILNET_V6: &str = "fd7a:115c:a1e0::/48";
/// Tailscale's fixed WireGuard port. Its client sends its encrypted packets
/// straight out the physical interface, not through our tunnel, so the direct
/// peer link (Mac to home) is passed by port. Relays (DERP, TCP 443 to many
/// changing hosts) are not; add the control server to the bypass list.
pub const TAILSCALE_PORT: u16 = 41641;
/// Most Tailscale relay/sign-in addresses one ruleset passes. Tailscale's
/// public relay map lists a few hundred.
pub const MAX_RELAYS: usize = 512;
/// Most addresses of "skip the VPN" websites one ruleset passes.
pub const MAX_SITE_ADDRS: usize = 256;

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
    ruleset_full(tunnel_if, endpoints, allow_lan, bypass, &[], &[])
}

/// Like [`ruleset_with_bypass`], also passing Tailscale's relay (DERP) and
/// sign-in servers in `relays`, only on the ports Tailscale uses for them:
/// TCP 80/443 (relayed WireGuard and control over HTTPS) and UDP 3478 (STUN).
/// Lets the real Tailscale app reach a tailnet when no direct path exists.
/// `sites` are addresses of websites the user chose to skip the VPN for;
/// they pass on web ports only (TCP 80/443 and QUIC on UDP 443).
pub fn ruleset_full(
    tunnel_if: Option<&str>,
    endpoints: &[SocketAddr],
    allow_lan: bool,
    bypass: &[IpNet],
    relays: &[IpAddr],
    sites: &[IpAddr],
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
    // A mesh VPN (Tailscale, TailFlow) running beside VPNFlow keeps working:
    // what it carries (tailnet addresses, MagicDNS) passes before the DNS
    // block, and so do its own encrypted packets on its fixed UDP port.
    r.push_str(&format!("pass quick inet to {TAILNET_V4}\n"));
    r.push_str(&format!("pass quick inet from {TAILNET_V4}\n"));
    r.push_str(&format!("pass quick inet6 to {TAILNET_V6}\n"));
    r.push_str(&format!("pass quick inet6 from {TAILNET_V6}\n"));
    r.push_str(&format!(
        "pass out quick proto udp from any port {TAILSCALE_PORT} to any\n"
    ));
    r.push_str(&format!(
        "pass in quick proto udp from any to any port {TAILSCALE_PORT}\n"
    ));
    for (fam, v4) in [("inet", true), ("inet6", false)] {
        let ips: Vec<String> = relays
            .iter()
            .filter(|a| a.is_ipv4() == v4)
            .take(MAX_RELAYS)
            .map(IpAddr::to_string)
            .collect();
        if ips.is_empty() {
            continue;
        }
        let list = ips.join(", ");
        r.push_str(&format!(
            "pass out quick {fam} proto tcp to {{ {list} }} port {{ 80, 443 }}\n"
        ));
        r.push_str(&format!(
            "pass out quick {fam} proto udp to {{ {list} }} port 3478\n"
        ));
    }
    for (fam, v4) in [("inet", true), ("inet6", false)] {
        let ips: Vec<String> = sites
            .iter()
            .filter(|a| a.is_ipv4() == v4)
            .take(MAX_SITE_ADDRS)
            .map(IpAddr::to_string)
            .collect();
        if !ips.is_empty() {
            r.push_str(&format!(
                "pass out quick {fam} proto {{ tcp, udp }} to {{ {} }} port {{ 80, 443 }}\n",
                ips.join(", ")
            ));
        }
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

    #[test]
    fn relays_pass_only_web_and_stun_ports() {
        let ep: SocketAddr = "203.0.113.9:51820".parse().unwrap();
        let relays: Vec<IpAddr> = vec![
            "199.38.181.104".parse().unwrap(),
            "2606:b740:49::104".parse().unwrap(),
            "192.200.0.101".parse().unwrap(),
        ];
        let r = ruleset_full(Some("utun10"), &[ep], false, &[], &relays, &[]).unwrap();
        assert!(r.contains(
            "pass out quick inet proto tcp to { 199.38.181.104, 192.200.0.101 } port { 80, 443 }\n"
        ));
        assert!(r.contains(
            "pass out quick inet proto udp to { 199.38.181.104, 192.200.0.101 } port 3478\n"
        ));
        assert!(r.contains(
            "pass out quick inet6 proto tcp to { 2606:b740:49::104 } port { 80, 443 }\n"
        ));
        // Still before the final block, and the block stays last.
        let pass = r.find("port 3478").unwrap();
        assert!(pass < r.find("block drop quick all").unwrap());
        assert!(r.ends_with("block drop quick all\n"));
    }

    #[test]
    fn sites_pass_only_web_ports_before_the_block() {
        let ep: SocketAddr = "203.0.113.9:51820".parse().unwrap();
        let sites: Vec<IpAddr> = vec![
            "198.51.100.7".parse().unwrap(),
            "2001:db8::7".parse().unwrap(),
        ];
        let r = ruleset_full(Some("utun10"), &[ep], false, &[], &[], &sites).unwrap();
        assert!(r.contains(
            "pass out quick inet proto { tcp, udp } to { 198.51.100.7 } port { 80, 443 }\n"
        ));
        assert!(r.contains(
            "pass out quick inet6 proto { tcp, udp } to { 2001:db8::7 } port { 80, 443 }\n"
        ));
        assert!(!r.contains("to 198.51.100.7\n"), "no all-port pass");
        let pass = r.find("198.51.100.7").unwrap();
        assert!(pass < r.find("block drop quick all").unwrap());
        assert!(r.ends_with("block drop quick all\n"));
    }

    #[test]
    fn no_relays_adds_nothing() {
        let ep: SocketAddr = "203.0.113.9:51820".parse().unwrap();
        let a = ruleset_full(None, &[ep], true, &[], &[], &[]).unwrap();
        let b = ruleset(None, &[ep], true).unwrap();
        assert_eq!(a, b);
        assert!(!a.contains("3478"));
    }

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
    fn tailnet_and_magicdns_pass_before_the_dns_block() {
        let r = ruleset(Some("utun4"), &[ep("203.0.113.7:51820")], false).unwrap();
        let pos = |needle: &str| r.find(needle).unwrap();
        let dns = pos("block drop quick proto { tcp, udp } from any to any port { 53, 853 }\n");
        for rule in [
            "pass quick inet to 100.64.0.0/10\n",
            "pass quick inet from 100.64.0.0/10\n",
            "pass quick inet6 to fd7a:115c:a1e0::/48\n",
            "pass quick inet6 from fd7a:115c:a1e0::/48\n",
        ] {
            assert!(pos(rule) < dns, "{rule}");
        }
        let block = pos("block drop quick all\n");
        for rule in [
            "pass out quick proto udp from any port 41641 to any\n",
            "pass in quick proto udp from any to any port 41641\n",
        ] {
            assert!(pos(rule) < block, "{rule}");
        }
        // The rest of the private ranges stay blocked unless allow_lan is on.
        assert!(!r.contains("192.168.0.0/16"));
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
