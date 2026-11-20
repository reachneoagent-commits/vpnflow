//! Live DNS and IPv6 checks for the running tunnel. The parsers and judges are
//! pure so they are unit-tested on any host; the probes in `lib.rs` feed them
//! real `scutil`, `route` and `ifconfig` output on every status poll.
//! Nothing here reports a protection as in effect unless the system state
//! says so right now.

use std::net::{IpAddr, Ipv4Addr};

use aegis_config::RouteKind;
use aegis_core::Feature;

/// Name servers of the resolvers that normal lookups use, from `scutil --dns`.
/// Scoped resolvers (only used by apps that bind to one interface) are skipped.
pub fn parse_resolvers(scutil_dns: &str) -> Vec<IpAddr> {
    let mut out: Vec<IpAddr> = Vec::new();
    let mut block: Vec<IpAddr> = Vec::new();
    let mut scoped = false;
    let mut flush = |block: &mut Vec<IpAddr>, scoped: &mut bool| {
        if !*scoped {
            for ip in block.drain(..) {
                if !out.contains(&ip) {
                    out.push(ip);
                }
            }
        }
        block.clear();
        *scoped = false;
    };
    for line in scutil_dns.lines().map(str::trim) {
        if line.starts_with("resolver #") || line.starts_with("DNS configuration") {
            flush(&mut block, &mut scoped);
        } else if line.starts_with("nameserver[") {
            if let Some(ip) = line
                .split(':')
                .skip(1)
                .collect::<Vec<_>>()
                .join(":")
                .trim()
                .split('%')
                .next()
                .and_then(|s| s.parse().ok())
            {
                block.push(ip);
            }
        } else if line.starts_with("flags") && line.contains("Scoped") {
            scoped = true;
        }
    }
    flush(&mut block, &mut scoped);
    out
}

/// `interface:` from `route -n get <dest>`; None when there is no route.
pub fn route_interface(route_get: &str) -> Option<String> {
    route_get
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("interface:"))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// True when a physical interface holds a global (non link-local) IPv6 address,
/// i.e. this network can actually carry IPv6. Reads `ifconfig` output.
pub fn has_native_ipv6(ifconfig: &str) -> bool {
    let mut iface = "";
    for line in ifconfig.lines() {
        if !line.starts_with(['\t', ' ']) {
            iface = line.split(':').next().unwrap_or("");
            continue;
        }
        if iface.starts_with("lo") || iface.starts_with("utun") {
            continue;
        }
        let Some(rest) = line.trim().strip_prefix("inet6 ") else {
            continue;
        };
        let addr = rest.split([' ', '%']).next().unwrap_or("");
        if let Ok(IpAddr::V6(a)) = addr.parse::<IpAddr>() {
            let link_local = a.segments()[0] & 0xffc0 == 0xfe80;
            if !link_local && !a.is_loopback() {
                return true;
            }
        }
    }
    false
}

/// Shown when the system has no DNS server at all, which is what a dropped
/// network looks like. Nothing can be verified, but nothing can leak either.
pub const NO_NETWORK: &str =
    "waiting for the network: no DNS server is set right now, so there is nothing to check";

/// Name looked up by the reachability check (reserved by RFC 2606).
pub const REACH_NAME: &str = "example.com";

/// A standard recursive DNS query (A record) for `name`.
pub fn dns_query(id: u16, name: &str) -> Vec<u8> {
    let mut q = Vec::with_capacity(17 + name.len());
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]);
    q
}

/// RCODE of a reply to query `id`, or `None` if it is not that reply.
pub fn dns_rcode(id: u16, reply: &[u8]) -> Option<u8> {
    let head = reply.get(..4)?;
    let is_reply = head[2] & 0x80 != 0;
    (u16::from_be_bytes([head[0], head[1]]) == id && is_reply).then_some(head[3] & 0x0f)
}

/// `rcode` is `None` when no matching reply arrived in time.
pub fn judge_reachability(rcode: Option<u8>) -> Feature {
    match rcode {
        Some(0 | 3) => Feature::Verified(format!(
            "the tunnel's DNS server answered a lookup for {REACH_NAME}"
        )),
        Some(code) => Feature::Failed(format!(
            "the tunnel's DNS server could not resolve names (DNS error {code}); the VPN server may have no internet access"
        )),
        None => Feature::Failed(
            "no answer from the tunnel's DNS server within 3 seconds; the connection may be down or the VPN server may have no internet access"
                .into(),
        ),
    }
}

/// Name whose authoritative answer is the querier's public IPv4. Queried only
/// through the tunnel's own DNS server, like the reachability check.
pub const WHOAMI_NAME: &str = "whoami.akamai.net";

fn skip_name(buf: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *buf.get(i)?;
        if len == 0 {
            return Some(i + 1);
        }
        if len & 0xc0 == 0xc0 {
            return Some(i + 2);
        }
        i = i.checked_add(1 + usize::from(len))?;
        if i > buf.len() {
            return None;
        }
    }
}

/// IPv4 addresses from the A answers in a reply to query `id`.
pub fn dns_a_records(id: u16, reply: &[u8]) -> Option<Vec<Ipv4Addr>> {
    if dns_rcode(id, reply)? != 0 {
        return Some(vec![]);
    }
    let ancount = u16::from_be_bytes([*reply.get(6)?, *reply.get(7)?]);
    let mut i = skip_name(reply, 12)?.checked_add(4)?;
    let mut out = Vec::new();
    for _ in 0..ancount {
        i = skip_name(reply, i)?;
        let rtype = u16::from_be_bytes([*reply.get(i)?, *reply.get(i + 1)?]);
        let rdlen = u16::from_be_bytes([*reply.get(i + 8)?, *reply.get(i + 9)?]) as usize;
        i = i.checked_add(10)?;
        let end = i.checked_add(rdlen)?;
        let rdata = reply.get(i..end)?;
        if rtype == 1 && rdata.len() == 4 {
            out.push(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]));
        }
        i = end;
    }
    Some(out)
}

pub fn judge_public_ip(addrs: Option<Vec<Ipv4Addr>>) -> Feature {
    match addrs {
        Some(list) if !list.is_empty() => Feature::Verified(list[0].to_string()),
        // Not a leak or a broken tunnel: many DNS servers simply do not answer
        // this question. Shown as "not available", never as a failure.
        Some(_) => Feature::Unsupported(
            "Not available: this DNS server does not report it. Your connection is not affected."
                .into(),
        ),
        None => Feature::Unsupported(
            "Not available: no answer within 3 seconds. Your connection is not affected.".into(),
        ),
    }
}

/// One query to `server`, waiting up to 3 s. Blocking. The kernel routes it,
/// so the caller must first check the route goes through the tunnel.
pub fn lookup(server: IpAddr) -> Option<u8> {
    let (id, reply) = exchange(server, REACH_NAME)?;
    dns_rcode(id, &reply)
}

/// Ask `server` for [`WHOAMI_NAME`] and return its A records. Blocking.
pub fn lookup_public_ip(server: IpAddr) -> Option<Vec<Ipv4Addr>> {
    let (id, reply) = exchange(server, WHOAMI_NAME)?;
    dns_a_records(id, &reply)
}

/// Second source when the profile's DNS does not answer [`WHOAMI_NAME`]:
/// OpenDNS answers `myip.opendns.com` with the address the question came from.
/// Only asked through the tunnel (the caller checks the route first).
pub const FALLBACK_SERVER: Ipv4Addr = Ipv4Addr::new(208, 67, 222, 222);
pub const FALLBACK_NAME: &str = "myip.opendns.com";

pub fn lookup_public_ip_fallback() -> Option<Vec<Ipv4Addr>> {
    let (id, reply) = exchange(IpAddr::V4(FALLBACK_SERVER), FALLBACK_NAME)?;
    dns_a_records(id, &reply)
}

fn exchange(server: IpAddr, name: &str) -> Option<(u16, Vec<u8>)> {
    use std::net::UdpSocket;
    use std::time::Duration;
    let id = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .subsec_nanos()
        & 0xffff) as u16;
    let sock = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })
    .ok()?;
    sock.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    sock.connect((server, 53)).ok()?;
    sock.send(&dns_query(id, name)).ok()?;
    let mut buf = [0u8; 512];
    let n = sock.recv(&mut buf).ok()?;
    dns_rcode(id, &buf[..n])?;
    Some((id, buf[..n].to_vec()))
}

/// `servers`: each system DNS server with the interface the kernel would use
/// to reach it right now.
pub fn judge_dns(servers: &[(IpAddr, Option<String>)], tunnel_iface: &str) -> Feature {
    if servers.is_empty() {
        return Feature::Configured(NO_NETWORK.into());
    }
    let outside: Vec<String> = servers
        .iter()
        .filter(|(_, i)| i.as_deref() != Some(tunnel_iface))
        .map(|(ip, i)| format!("{ip} via {}", i.as_deref().unwrap_or("no route")))
        .collect();
    if outside.is_empty() {
        let n = servers.len();
        Feature::Verified(format!(
            "all {n} system DNS server{} reached through the tunnel",
            if n == 1 { " is" } else { "s are" }
        ))
    } else {
        Feature::Failed(format!(
            "DNS lookups can leave outside the tunnel: {}",
            outside.join(", ")
        ))
    }
}

/// `v6_route_iface`: interface for an IPv6 internet address, None = no route.
pub fn judge_ipv6(
    route: RouteKind,
    v6_route_iface: Option<&str>,
    native_v6: bool,
    tunnel_iface: &str,
) -> Feature {
    if matches!(route, RouteKind::FullIpv6Only | RouteKind::Split) {
        return Feature::Off;
    }
    match v6_route_iface {
        Some(i) if i == tunnel_iface && route == RouteKind::Full => {
            Feature::Verified("IPv6 internet traffic is routed through the tunnel".into())
        }
        Some(i) if i == tunnel_iface => Feature::Verified(
            "IPv6 is sent into the tunnel, which does not carry it, so it is dropped".into(),
        ),
        None => Feature::Verified("there is no IPv6 route, so IPv6 cannot leave".into()),
        Some(_) if !native_v6 => Feature::Verified(
            "this network has no global IPv6 address, so there is nothing to leak".into(),
        ),
        Some(i) => Feature::Failed(format!(
            "IPv6 traffic leaves outside the tunnel (via {i}). Use a profile with IPv6 or turn IPv6 off for this network."
        )),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn whoami_answer_is_parsed_and_judged() {
        let mut reply = super::dns_query(0x1111, "whoami.akamai.net");
        reply[2] |= 0x80;
        reply[3] = 0x80;
        reply[7] = 1;
        reply.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 203, 0, 113, 50]);
        let a = super::dns_a_records(0x1111, &reply);
        assert_eq!(a, Some(vec![std::net::Ipv4Addr::new(203, 0, 113, 50)]));
        assert!(
            matches!(super::judge_public_ip(a), aegis_core::Feature::Verified(s) if s == "203.0.113.50")
        );
        assert!(matches!(
            super::judge_public_ip(Some(vec![])),
            aegis_core::Feature::Unsupported(_)
        ));
        assert!(matches!(
            super::judge_public_ip(None),
            aegis_core::Feature::Unsupported(_)
        ));
        assert_eq!(super::dns_a_records(0x2222, &reply), None);
    }

    use super::*;

    const SCUTIL: &str = "DNS configuration\n\nresolver #1\n  nameserver[0] : 10.2.0.1\n  flags    : Request A records\n\nresolver #2\n  domain   : local\n  options  : mdns\n\nDNS configuration (for scoped queries)\n\nresolver #1\n  nameserver[0] : 192.168.1.1\n  if_index : 14 (en0)\n  flags    : Scoped, Request A records\n";

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn resolvers_skip_scoped() {
        assert_eq!(parse_resolvers(SCUTIL), vec![ip("10.2.0.1")]);
    }

    #[test]
    fn route_iface_parse() {
        assert_eq!(
            route_interface("   gateway: 10.2.0.2\n  interface: utun8\n").as_deref(),
            Some("utun8")
        );
        assert_eq!(route_interface("route: not in table\n"), None);
    }

    #[test]
    fn native_ipv6_detection() {
        let only_ll = "lo0: flags=8049\n\tinet6 ::1 prefixlen 128\nen0: flags=8863\n\tinet6 fe80::1%en0 prefixlen 64 scopeid 0xe\nutun8: flags=8051\n\tinet6 2001:db8::1 prefixlen 64\n";
        assert!(!has_native_ipv6(only_ll));
        let global =
            "en0: flags=8863\n\tinet6 fe80::1%en0 prefixlen 64\n\tinet6 2001:db8::5 prefixlen 64\n";
        assert!(has_native_ipv6(global));
    }

    #[test]
    fn dns_verdicts() {
        let t = "utun8";
        let ok = [(ip("10.2.0.1"), Some("utun8".to_string()))];
        assert!(matches!(judge_dns(&ok, t), Feature::Verified(_)));
        let leak = [
            (ip("10.2.0.1"), Some("utun8".to_string())),
            (ip("192.168.1.1"), Some("en0".to_string())),
        ];
        assert!(matches!(judge_dns(&leak, t), Feature::Failed(_)));
        assert!(matches!(judge_dns(&[], t), Feature::Configured(_)));
        let none = [(ip("9.9.9.9"), None)];
        assert!(matches!(judge_dns(&none, t), Feature::Failed(_)));
    }

    #[test]
    fn ipv6_verdicts() {
        let t = "utun8";
        let v = |f: Feature| matches!(f, Feature::Verified(_));
        assert!(v(judge_ipv6(RouteKind::Full, Some("utun8"), true, t)));
        assert!(v(judge_ipv6(
            RouteKind::FullIpv4Only,
            Some("utun8"),
            true,
            t
        )));
        assert!(v(judge_ipv6(RouteKind::FullIpv4Only, None, true, t)));
        assert!(v(judge_ipv6(
            RouteKind::FullIpv4Only,
            Some("en0"),
            false,
            t
        )));
        assert!(matches!(
            judge_ipv6(RouteKind::FullIpv4Only, Some("en0"), true, t),
            Feature::Failed(_)
        ));
        assert_eq!(
            judge_ipv6(RouteKind::Split, Some("en0"), true, t),
            Feature::Off
        );
    }

    #[test]
    fn dns_query_and_reachability() {
        let q = dns_query(0x1234, "example.com");
        assert_eq!(&q[..2], &[0x12, 0x34]);
        assert_eq!(&q[12..25], b"\x07example\x03com\x00");
        let reply = [0x12, 0x34, 0x81, 0x80];
        assert_eq!(dns_rcode(0x1234, &reply), Some(0));
        assert_eq!(dns_rcode(0x9999, &reply), None);
        assert_eq!(dns_rcode(0x1234, &[0x12, 0x34, 0x01, 0x00]), None);
        assert!(matches!(judge_reachability(Some(0)), Feature::Verified(_)));
        assert!(matches!(judge_reachability(Some(3)), Feature::Verified(_)));
        assert!(matches!(judge_reachability(Some(2)), Feature::Failed(_)));
        assert!(matches!(judge_reachability(None), Feature::Failed(_)));
    }
}
