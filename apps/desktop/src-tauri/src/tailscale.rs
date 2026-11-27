//! "Let Tailscale through": while the kill switch blocks, the real Tailscale
//! app may still reach its relay (DERP) and sign-in servers, so a tailnet
//! keeps working when no direct path to a peer exists.
//!
//! Tailscale publishes its relay map at a fixed URL. We fetch it (through the
//! VPN, like any other app traffic), add the sign-in servers' addresses, keep
//! a copy on disk, and hand the list to the daemon, which passes only those
//! addresses on TCP 80/443 and UDP 3478. Turning the setting off sends an
//! empty list.
//!
//! DNS: with Tailscale's "Use Tailscale DNS" on, 100.100.100.100 answers every
//! lookup on the Mac and forwards ordinary names outside the tunnel, which the
//! kill switch blocks, so all DNS stops. While connected we turn that option
//! off with Tailscale's own CLI (remembering that we did) and ask the daemon
//! for `/etc/resolver/<tailnet domain>`, so only tailnet names go to
//! Tailscale. On disconnect, or when the setting goes off, both are undone.

use std::collections::BTreeSet;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_core::{ConnectionState, Engine};

use crate::backend::AnyBackend;

const DERP_MAP_URL: &str = "https://controlplane.tailscale.com/derpmap/default";
/// Tailscale's own servers the app talks to besides the relays.
const CONTROL_HOSTS: &[&str] = &[
    "controlplane.tailscale.com",
    "login.tailscale.com",
    "log.tailscale.com",
];
const CACHE_FILE: &str = "tailscale-relays.json";
/// How often the setting is checked. No network traffic on its own.
const POLL: Duration = Duration::from_secs(5);
/// Relay addresses rarely change; refetch this often.
const REFRESH: Duration = Duration::from_secs(6 * 60 * 60);
/// After a failed fetch, try again this soon.
const RETRY: Duration = Duration::from_secs(5 * 60);
/// Resend the list this often, so a restarted daemon gets it back quickly
/// (the daemon ignores a list it already has).
const RESEND: Duration = Duration::from_secs(60);
/// While connected, re-check Tailscale's DNS option this often (the user or
/// Tailscale may turn it back on).
const DNS_RECHECK: Duration = Duration::from_secs(60);
/// Present while we have turned Tailscale DNS off and still owe it back.
const DNS_MARKER: &str = "tailscale-dns-off";
/// Where Tailscale's command line lives on a Mac.
const TS_CLIS: &[&str] = &[
    "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    "/usr/local/bin/tailscale",
    "/opt/homebrew/bin/tailscale",
];
/// Same cap as the daemon's.
const MAX_ADDRS: usize = 512;

/// Relay addresses from Tailscale's DERP map JSON
/// (`Regions.<id>.Nodes[].IPv4/IPv6`). Unusable entries are skipped.
pub fn parse_derp_map(json: &serde_json::Value) -> BTreeSet<IpAddr> {
    let mut out = BTreeSet::new();
    let Some(regions) = json.get("Regions").and_then(|r| r.as_object()) else {
        return out;
    };
    for region in regions.values() {
        let Some(nodes) = region.get("Nodes").and_then(|n| n.as_array()) else {
            continue;
        };
        for node in nodes {
            for key in ["IPv4", "IPv6"] {
                if let Some(ip) = node
                    .get(key)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<IpAddr>().ok())
                    .filter(usable)
                {
                    out.insert(ip);
                }
            }
        }
    }
    out
}

fn usable(ip: &IpAddr) -> bool {
    !(ip.is_unspecified() || ip.is_loopback() || ip.is_multicast())
}

fn fetch() -> Result<Vec<IpAddr>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut resp = agent
        .get(DERP_MAP_URL)
        .header("User-Agent", "VPNFlow")
        .call()
        .map_err(|e| format!("relay map fetch failed: {e}"))?;
    let v: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("relay map unreadable: {e}"))?;
    let mut set = parse_derp_map(&v);
    if set.is_empty() {
        return Err("relay map had no addresses".into());
    }
    for host in CONTROL_HOSTS {
        if let Ok(addrs) = (*host, 443).to_socket_addrs() {
            set.extend(addrs.map(|a| a.ip()).filter(usable));
        }
    }
    Ok(set.into_iter().take(MAX_ADDRS).collect())
}

fn load_cache(path: &Path) -> Vec<IpAddr> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<IpAddr>>(&b).ok())
        .map(|v| v.into_iter().filter(usable).take(MAX_ADDRS).collect())
        .unwrap_or_default()
}

fn save_cache(path: &Path, addrs: &[IpAddr]) {
    if let Ok(b) = serde_json::to_vec(addrs)
        && let Err(e) = std::fs::write(path, b)
    {
        tracing::warn!("could not save the Tailscale relay list: {e}");
    }
}

/// The tailnet's DNS domain from `tailscale status --json`.
pub fn magic_dns_suffix(status: &serde_json::Value) -> Option<String> {
    let raw = status
        .pointer("/CurrentTailnet/MagicDNSSuffix")
        .or_else(|| status.get("MagicDNSSuffix"))
        .and_then(|v| v.as_str())?;
    let d = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    (d.contains('.') && !d.is_empty()).then_some(d)
}

/// Whether "Use Tailscale DNS" is on, from `tailscale debug prefs`.
pub fn corp_dns(prefs: &serde_json::Value) -> Option<bool> {
    prefs.get("CorpDNS").and_then(|v| v.as_bool())
}

fn ts_cli() -> Option<&'static str> {
    TS_CLIS.iter().copied().find(|p| Path::new(p).exists())
}

fn ts_json(cli: &str, args: &[&str]) -> Option<serde_json::Value> {
    let out = std::process::Command::new(cli).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

fn ts_set_accept_dns(cli: &str, on: bool) -> bool {
    let flag = if on {
        "--accept-dns=true"
    } else {
        "--accept-dns=false"
    };
    match std::process::Command::new(cli).args(["set", flag]).output() {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            tracing::warn!(
                "tailscale set {flag} failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            false
        }
        Err(e) => {
            tracing::warn!("could not run tailscale: {e}");
            false
        }
    }
}

/// One DNS pass. Returns the tailnet domain the daemon should route to
/// Tailscale, or `None` for no resolver file.
fn dns_step(dir: &Path, want: bool) -> Option<String> {
    let marker = dir.join(DNS_MARKER);
    let cli = ts_cli()?;
    if !want {
        if marker.exists() && ts_set_accept_dns(cli, true) {
            let _ = std::fs::remove_file(&marker);
        }
        return None;
    }
    let domain = magic_dns_suffix(&ts_json(cli, &["status", "--json"])?)?;
    if corp_dns(&ts_json(cli, &["debug", "prefs"])?) == Some(true) {
        // Write the marker first: if we crash right after, the next start
        // still knows to turn it back on.
        if let Err(e) = std::fs::write(&marker, b"") {
            tracing::warn!("could not save the Tailscale DNS marker: {e}");
            return None;
        }
        if !ts_set_accept_dns(cli, false) {
            let _ = std::fs::remove_file(&marker);
            return None;
        }
    }
    Some(domain)
}

pub fn spawn(engine: Arc<Engine<AnyBackend>>, dir: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let cache = dir.join(CACHE_FILE);
        let mut addrs = load_cache(&cache);
        let mut next_fetch = Instant::now();
        // None = nothing sent yet, so the first pass always tells the daemon.
        let mut sent: Option<(Vec<IpAddr>, Instant)> = None;
        // (wanted, domain sent to the daemon, when checked)
        let mut dns: Option<(bool, Option<String>, Instant)> = None;
        loop {
            let on = engine.settings().kill_switch_allow_tailscale;
            let want_dns = on && !matches!(engine.snapshot().state, ConnectionState::Disconnected);
            let dns_due = match &dns {
                None => true,
                Some((w, _, at)) => *w != want_dns || (want_dns && at.elapsed() >= DNS_RECHECK),
            };
            if dns_due {
                let d = dir.clone();
                let domain = tauri::async_runtime::spawn_blocking(move || dns_step(&d, want_dns))
                    .await
                    .ok()
                    .flatten();
                let unchanged = dns.as_ref().is_some_and(|(_, sent, _)| *sent == domain);
                if unchanged {
                    dns = Some((want_dns, domain, Instant::now()));
                } else {
                    match engine.backend().apply_tailnet_dns(domain.as_deref()).await {
                        Ok(()) => dns = Some((want_dns, domain, Instant::now())),
                        Err(e) => tracing::debug!("could not set tailnet DNS: {e}"),
                    }
                }
            }
            if on && Instant::now() >= next_fetch {
                match tauri::async_runtime::spawn_blocking(fetch).await {
                    Ok(Ok(fresh)) => {
                        save_cache(&cache, &fresh);
                        addrs = fresh;
                        next_fetch = Instant::now() + REFRESH;
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("{e}");
                        next_fetch = Instant::now() + RETRY;
                    }
                    Err(e) => {
                        tracing::warn!("relay map task failed: {e}");
                        next_fetch = Instant::now() + RETRY;
                    }
                }
            }
            let want: Vec<IpAddr> = if on { addrs.clone() } else { Vec::new() };
            let due = match &sent {
                None => true,
                Some((list, at)) => *list != want || (on && at.elapsed() >= RESEND),
            };
            if due {
                match engine.backend().apply_tailscale_relays(&want).await {
                    Ok(()) => sent = Some((want, Instant::now())),
                    // Daemon busy or restarting: try again on the next pass.
                    Err(e) => tracing::debug!("could not send the Tailscale relay list: {e}"),
                }
            }
            tokio::time::sleep(POLL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relay_addresses() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"Regions":{
                "1":{"Nodes":[{"HostName":"derp1","IPv4":"199.38.181.104","IPv6":"2606:b740:49::104"}]},
                "2":{"Nodes":[{"HostName":"derp2","IPv4":"bad"},{"IPv4":"0.0.0.0"},{"IPv4":"192.200.0.101"}]},
                "3":{}
            }}"#,
        )
        .unwrap();
        let got: Vec<String> = parse_derp_map(&v).iter().map(ToString::to_string).collect();
        assert_eq!(
            got,
            ["192.200.0.101", "199.38.181.104", "2606:b740:49::104"]
        );
    }

    #[test]
    fn reads_tailnet_domain_and_dns_option() {
        let st = serde_json::json!({"CurrentTailnet":{"MagicDNSSuffix":"Tail1234.TS.net."}});
        assert_eq!(magic_dns_suffix(&st).as_deref(), Some("tail1234.ts.net"));
        assert_eq!(
            magic_dns_suffix(&serde_json::json!({"MagicDNSSuffix":"home.example"})).as_deref(),
            Some("home.example")
        );
        assert_eq!(magic_dns_suffix(&serde_json::json!({})), None);
        assert_eq!(
            magic_dns_suffix(&serde_json::json!({"MagicDNSSuffix":""})),
            None
        );
        assert_eq!(corp_dns(&serde_json::json!({"CorpDNS":true})), Some(true));
        assert_eq!(corp_dns(&serde_json::json!({})), None);
    }

    #[test]
    fn empty_or_odd_json_gives_nothing() {
        assert!(parse_derp_map(&serde_json::json!({})).is_empty());
        assert!(parse_derp_map(&serde_json::json!({"Regions": []})).is_empty());
    }

    #[test]
    fn cache_round_trip_drops_bad_entries() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(CACHE_FILE);
        save_cache(
            &p,
            &["1.2.3.4".parse().unwrap(), "127.0.0.1".parse().unwrap()],
        );
        assert_eq!(load_cache(&p), vec!["1.2.3.4".parse::<IpAddr>().unwrap()]);
        assert!(load_cache(&dir.path().join("missing")).is_empty());
    }
}
