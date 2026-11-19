//! Sleep/wake and network-change detection for macOS, by light polling.
//! No extra dependencies: wake is seen as wall-clock time jumping ahead of the
//! monotonic clock (which stops during sleep); a network change is seen as a
//! different set of non-tunnel addresses or default routes.

use std::time::Duration;

pub const POLL: Duration = Duration::from_secs(2);
/// Wall time may exceed monotonic time by this much before it counts as a sleep.
pub const SLEEP_GAP: Duration = Duration::from_secs(5);

/// True when the computer slept between two polls.
pub fn slept(wall: Duration, mono: Duration) -> bool {
    wall.saturating_sub(mono) >= SLEEP_GAP
}

fn is_tunnel_or_virtual(name: &str) -> bool {
    [
        "utun", "lo", "awdl", "llw", "bridge", "gif", "stf", "ap", "anpi",
    ]
    .iter()
    .any(|p| name.starts_with(p))
}

/// A fingerprint of how this Mac reaches the network, ignoring VPN tunnels.
/// Equal fingerprints mean "same network"; any difference means it changed.
pub fn network_signature(ifconfig: &str, netstat: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut iface = String::new();
    for line in ifconfig.lines() {
        if !line.starts_with([' ', '\t']) {
            iface = line.split(':').next().unwrap_or("").to_owned();
            continue;
        }
        if is_tunnel_or_virtual(&iface) {
            continue;
        }
        let t = line.trim();
        if t.starts_with("inet ") || t.starts_with("status:") {
            parts.push(format!("{iface} {t}"));
        }
    }
    for line in netstat.lines().filter(|l| l.starts_with("default")) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if let Some(last) = cols.last()
            && !is_tunnel_or_virtual(last)
        {
            parts.push(format!("route {}", cols.join(" ")));
        }
    }
    parts.sort();
    parts.join("\n")
}

#[cfg(target_os = "macos")]
fn run(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Starts the watcher thread and returns the receiving end.
#[cfg(target_os = "macos")]
pub fn spawn() -> tokio::sync::mpsc::Receiver<aegis_core::SystemEvent> {
    use aegis_core::SystemEvent;
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    std::thread::spawn(move || {
        let sig = || {
            network_signature(
                &run("/sbin/ifconfig", &[]),
                &run("/usr/sbin/netstat", &["-rn", "-f", "inet"]),
            )
        };
        let mut last_sig = sig();
        let mut last_wall = std::time::SystemTime::now();
        let mut last_mono = std::time::Instant::now();
        loop {
            std::thread::sleep(POLL);
            let (wall, mono) = (std::time::SystemTime::now(), std::time::Instant::now());
            let wall_d = wall.duration_since(last_wall).unwrap_or_default();
            let mono_d = mono.duration_since(last_mono);
            (last_wall, last_mono) = (wall, mono);
            let mut ev = None;
            if slept(wall_d, mono_d) {
                ev = Some(SystemEvent::Resumed);
            }
            let now = sig();
            if now != last_sig {
                last_sig = now;
                ev = ev.or(Some(SystemEvent::NetworkChanged));
            }
            if let Some(e) = ev
                && tx.blocking_send(e).is_err()
            {
                return; // engine gone
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sleep_gap() {
        assert!(!slept(Duration::from_secs(2), Duration::from_secs(2)));
        assert!(!slept(Duration::from_secs(3), Duration::from_secs(2)));
        assert!(slept(Duration::from_secs(600), Duration::from_secs(2)));
    }

    #[test]
    fn signature_ignores_tunnels_and_tracks_wifi() {
        let a = "lo0: flags=8049\n\tinet 127.0.0.1\nen0: flags=8863\n\tinet 192.168.1.20 netmask 0xffffff00\n\tstatus: active\nutun4: flags=8051\n\tinet 10.2.0.2\n";
        let tun_changed = a.replace("10.2.0.2", "10.2.0.9");
        let wifi_changed = a.replace("192.168.1.20", "172.20.10.4");
        let ns = "default 192.168.1.1 UGScg en0\ndefault link#25 UCSIg utun4\n";
        assert_eq!(
            network_signature(a, ns),
            network_signature(&tun_changed, ns)
        );
        assert_ne!(
            network_signature(a, ns),
            network_signature(&wifi_changed, ns)
        );
        let off = a.replace("status: active", "status: inactive");
        assert_ne!(network_signature(a, ns), network_signature(&off, ns));
    }
}
