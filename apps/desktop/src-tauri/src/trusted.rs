//! Trusted networks: skip the VPN on networks the user marked as trusted and
//! connect everywhere else. The network is identified by the router's hardware
//! address, which macOS gives to any app (Wi-Fi names are hidden without a
//! location permission). It acts only when the network *changes*, so a manual
//! choice made on the current network is never overridden.

use std::sync::Arc;
use std::time::Duration;

use aegis_core::{Engine, Settings};

use crate::backend::AnyBackend;

const POLL: Duration = Duration::from_secs(3);
/// A new network gets this long to settle before we act on it.
const SETTLE: Duration = Duration::from_secs(2);

/// What to do after the network changed.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Disconnect,
    Connect,
}

/// The router address (`aa:bb:cc:dd:ee:ff`) from `route -n get default` and
/// `arp -n <gateway>` output. None when there is no usable default route.
pub fn parse_router_id(route: &str, arp: &str) -> Option<String> {
    let _ = route;
    let at = arp.split(" at ").nth(1)?;
    let mac = at.split_whitespace().next()?.to_ascii_lowercase();
    let ok = mac.split(':').count() == 6
        && mac
            .split(':')
            .all(|p| !p.is_empty() && p.len() <= 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
    ok.then(|| {
        // arp prints "9:..." without leading zeros; normalise to two digits.
        mac.split(':')
            .map(|p| format!("{p:0>2}"))
            .collect::<Vec<_>>()
            .join(":")
    })
}

pub fn gateway_of(route: &str) -> Option<String> {
    route
        .lines()
        .find_map(|l| l.trim().strip_prefix("gateway:"))
        .map(|g| g.trim().to_owned())
        .filter(|g| !g.is_empty())
}

/// Decide what the new network calls for.
pub fn decide(s: &Settings, network: Option<&str>, idle: bool) -> Action {
    if !s.trusted_networks_enabled {
        return Action::Nothing;
    }
    let Some(id) = network else {
        return Action::Nothing; // offline: nothing to protect or skip yet
    };
    let trusted = s.trusted_networks.iter().any(|n| n.id == id);
    match (trusted, idle) {
        (true, false) => Action::Disconnect,
        (false, true) => Action::Connect,
        _ => Action::Nothing,
    }
}

#[cfg(target_os = "macos")]
pub fn current_network_id() -> Option<String> {
    let run = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    // Ask for the route as it would be without the tunnel: the physical
    // interface's own gateway, so a connected VPN does not hide the router.
    let route = run("/sbin/route", &["-n", "get", "default", "-ifscope", "en0"]);
    let route = if gateway_of(&route).is_some() {
        route
    } else {
        run("/sbin/route", &["-n", "get", "default"])
    };
    let gw = gateway_of(&route)?;
    parse_router_id(&route, &run("/usr/sbin/arp", &["-n", &gw]))
}

#[cfg(not(target_os = "macos"))]
pub fn current_network_id() -> Option<String> {
    None
}

pub fn spawn(engine: Arc<Engine<AnyBackend>>) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<Option<String>> = None;
        loop {
            tokio::time::sleep(POLL).await;
            let now = tokio::task::spawn_blocking(current_network_id)
                .await
                .unwrap_or(None);
            if last.as_ref() == Some(&now) {
                continue;
            }
            let first = last.is_none();
            last = Some(now.clone());
            if first {
                // At launch the "connect on launch" setting decides, not this.
                continue;
            }
            tokio::time::sleep(SETTLE).await;
            let s = engine.settings();
            let snap = engine.snapshot();
            match decide(&s, now.as_deref(), snap.state.is_idle()) {
                Action::Nothing => {}
                Action::Disconnect => {
                    let _ = engine.disconnect().await;
                }
                Action::Connect => {
                    let _ = match s.preferred_profile {
                        Some(id) => engine.connect(id).await,
                        None => engine.quick_connect().await.map(|_| ()),
                    };
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::TrustedNetwork;

    fn settings(on: bool) -> Settings {
        Settings {
            trusted_networks_enabled: on,
            trusted_networks: vec![TrustedNetwork {
                id: "9c:53:22:c9:5c:44".into(),
                name: "Home".into(),
            }],
            ..Settings::default()
        }
    }

    #[test]
    fn reads_router_address() {
        let arp = "? (192.168.10.1) at 9c:53:22:c9:5c:44 on en0 ifscope [ethernet]";
        assert_eq!(
            parse_router_id("", arp).as_deref(),
            Some("9c:53:22:c9:5c:44")
        );
        let short = "? (10.0.0.1) at 0:1a:2b:3:4:5 on en0 ifscope [ethernet]";
        assert_eq!(
            parse_router_id("", short).as_deref(),
            Some("00:1a:2b:03:04:05")
        );
        assert_eq!(
            parse_router_id("", "? (10.0.0.9) at (incomplete) on en0"),
            None
        );
        assert_eq!(parse_router_id("", ""), None);
    }

    #[test]
    fn reads_gateway() {
        assert_eq!(
            gateway_of("   gateway: 192.168.10.1\n interface: en0").as_deref(),
            Some("192.168.10.1")
        );
        assert_eq!(gateway_of("route: writing to routing socket"), None);
    }

    #[test]
    fn trusted_network_turns_vpn_off_others_turn_it_on() {
        let home = Some("9c:53:22:c9:5c:44");
        let cafe = Some("aa:bb:cc:dd:ee:ff");
        assert_eq!(decide(&settings(true), home, false), Action::Disconnect);
        assert_eq!(decide(&settings(true), home, true), Action::Nothing);
        assert_eq!(decide(&settings(true), cafe, true), Action::Connect);
        assert_eq!(decide(&settings(true), cafe, false), Action::Nothing);
        assert_eq!(decide(&settings(true), None, true), Action::Nothing);
        assert_eq!(decide(&settings(false), cafe, true), Action::Nothing);
    }
}
