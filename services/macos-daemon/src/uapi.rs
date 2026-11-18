//! wireguard-go's userspace configuration protocol (UAPI): build the `set=1`
//! request from a validated config and parse the `get=1` reply. Pure, so it is
//! unit-tested here. Key material only ever appears in the returned
//! `Zeroizing` string, which goes straight to the root-only UAPI socket.

use std::fmt::Write;
use std::net::SocketAddr;

use aegis_config::WgConfig;
use zeroize::Zeroizing;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// `endpoints[i]` is the already-resolved address of peer `i` (None: no endpoint).
pub fn set_request(cfg: &WgConfig, endpoints: &[Option<SocketAddr>]) -> Zeroizing<String> {
    let mut r = Zeroizing::new(String::from("set=1\n"));
    let _ = writeln!(
        r,
        "private_key={}",
        hex(cfg.interface.private_key.expose_bytes())
    );
    if let Some(p) = cfg.interface.listen_port {
        let _ = writeln!(r, "listen_port={p}");
    }
    r.push_str("replace_peers=true\n");
    for (i, peer) in cfg.peers.iter().enumerate() {
        let _ = writeln!(r, "public_key={}", hex(peer.public_key.as_bytes()));
        if let Some(psk) = &peer.preshared_key {
            let _ = writeln!(r, "preshared_key={}", hex(psk.expose_bytes()));
        }
        if let Some(Some(ep)) = endpoints.get(i) {
            let _ = writeln!(r, "endpoint={ep}");
        }
        if let Some(k) = peer.persistent_keepalive {
            let _ = writeln!(r, "persistent_keepalive_interval={k}");
        }
        r.push_str("replace_allowed_ips=true\n");
        for net in &peer.allowed_ips {
            let _ = writeln!(r, "allowed_ip={net}");
        }
    }
    r.push('\n');
    r
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Unix ms of the newest handshake across peers.
    pub last_handshake_ms: Option<u64>,
}

pub fn parse_get(reply: &str) -> Stats {
    let mut s = Stats::default();
    let mut sec = 0u64;
    let mut nsec = 0u64;
    let mut newest: Option<u64> = None;
    let flush = |sec: &mut u64, nsec: &mut u64, newest: &mut Option<u64>| {
        if *sec > 0 {
            let ms = *sec * 1000 + *nsec / 1_000_000;
            *newest = Some(newest.map_or(ms, |n| n.max(ms)));
        }
        *sec = 0;
        *nsec = 0;
    };
    for line in reply.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k {
            "public_key" => flush(&mut sec, &mut nsec, &mut newest),
            "rx_bytes" => s.rx_bytes += v.parse().unwrap_or(0),
            "tx_bytes" => s.tx_bytes += v.parse().unwrap_or(0),
            "last_handshake_time_sec" => sec = v.parse().unwrap_or(0),
            "last_handshake_time_nsec" => nsec = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    flush(&mut sec, &mut nsec, &mut newest);
    s.last_handshake_ms = newest;
    s
}

/// The reply to `set=1` ends with `errno=N`; 0 is success.
pub fn set_succeeded(reply: &str) -> bool {
    reply.lines().any(|l| l.trim() == "errno=0")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "[Interface]\nPrivateKey = AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=\nAddress = 10.2.0.2/32, fd00::2/128\nDNS = 10.2.0.1\n\n[Peer]\nPublicKey = AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = 203.0.113.7:51820\nPersistentKeepalive = 25\n";

    #[test]
    fn builds_a_request() {
        let cfg = aegis_config::parse(CONF).unwrap();
        let ep: SocketAddr = "203.0.113.7:51820".parse().unwrap();
        let r = set_request(&cfg, &[Some(ep)]);
        assert!(r.starts_with("set=1\nprivate_key="));
        assert!(r.contains("replace_peers=true\n"));
        assert!(r.contains("endpoint=203.0.113.7:51820\n"));
        assert!(r.contains("persistent_keepalive_interval=25\n"));
        assert!(r.contains("allowed_ip=0.0.0.0/0\n"));
        assert!(r.contains("allowed_ip=::/0\n"));
        assert!(r.ends_with("\n\n"));
        // 32 bytes of key -> 64 hex chars
        let line = r.lines().nth(1).unwrap();
        assert_eq!(line.len(), "private_key=".len() + 64);
    }

    #[test]
    fn parses_stats_and_newest_handshake() {
        let reply = "private_key=aa\npublic_key=aa\nrx_bytes=10\ntx_bytes=5\nlast_handshake_time_sec=100\nlast_handshake_time_nsec=500000000\npublic_key=bb\nrx_bytes=1\ntx_bytes=1\nlast_handshake_time_sec=200\nlast_handshake_time_nsec=0\nerrno=0\n\n";
        let s = parse_get(reply);
        assert_eq!(s.rx_bytes, 11);
        assert_eq!(s.tx_bytes, 6);
        assert_eq!(s.last_handshake_ms, Some(200_000));
        assert!(set_succeeded(reply));
        assert!(!set_succeeded("errno=1\n"));
    }

    #[test]
    fn no_handshake_is_none() {
        let s = parse_get("public_key=aa\nlast_handshake_time_sec=0\nlast_handshake_time_nsec=0\n");
        assert_eq!(s.last_handshake_ms, None);
    }
}
