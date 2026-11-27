//! "Skip the VPN for these sites": websites that block VPN users reach the
//! user over the normal connection instead.
//!
//! While connected we look each site up (and its `www.` name) with the
//! system resolver, which then answers through the VPN, so the lookups do not
//! leak. The daemon routes the answers around the tunnel and lets the kill
//! switch pass them on web ports. Sites spread over many changing addresses,
//! so we look again every 20 seconds and keep every address seen during the
//! connection (up to a cap). Disconnecting or emptying the list sends an
//! empty list, which removes the routes.

use std::collections::BTreeSet;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_core::{ConnectionState, Engine};

use crate::backend::AnyBackend;

/// How often the state and setting are checked. No network traffic on its own.
const POLL: Duration = Duration::from_secs(3);
/// Look the sites up again this often while connected. CDNs rotate between
/// address sets about once a minute (Akamai answers with a 60 s TTL), and the
/// browser uses whichever set is current, so we need to see each set soon
/// after it appears. The lookups are answered from the system's DNS cache
/// most of the time.
const RELOOKUP: Duration = Duration::from_secs(20);
/// Resend the list this often, so a restarted daemon gets it back quickly.
const RESEND: Duration = Duration::from_secs(60);
/// Same cap as the daemon's.
const MAX_ADDRS: usize = 256;

/// Names to look up for the saved sites: each one and its `www.` form.
pub fn lookup_names(sites: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for s in sites {
        if let Ok(h) = aegis_core::parse_bypass_site(s) {
            out.push(format!("www.{h}"));
            out.push(h);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn lookup(names: &[String]) -> BTreeSet<IpAddr> {
    let mut out = BTreeSet::new();
    for n in names {
        match (n.as_str(), 443).to_socket_addrs() {
            Ok(addrs) => out.extend(addrs.map(|a| a.ip()).filter(aegis_core::public_site_addr)),
            Err(e) => tracing::debug!("could not look up a skipped site: {e}"),
        }
    }
    out
}

pub fn spawn(engine: Arc<Engine<AnyBackend>>) {
    tauri::async_runtime::spawn(async move {
        // Names the current set of addresses was built for.
        let mut names: Vec<String> = Vec::new();
        let mut addrs: BTreeSet<IpAddr> = BTreeSet::new();
        let mut next_lookup = Instant::now();
        // None = nothing sent yet, so the first pass always tells the daemon.
        let mut sent: Option<(Vec<IpAddr>, Instant)> = None;
        loop {
            let connected = matches!(engine.snapshot().state, ConnectionState::Connected { .. });
            let want_names = if connected {
                lookup_names(&engine.settings().bypass_sites)
            } else {
                Vec::new()
            };
            if want_names != names {
                // New connection or the list changed: start over.
                names = want_names;
                addrs.clear();
                next_lookup = Instant::now();
            }
            if !names.is_empty() && Instant::now() >= next_lookup {
                let n = names.clone();
                if let Ok(found) = tauri::async_runtime::spawn_blocking(move || lookup(&n)).await {
                    for ip in found {
                        if addrs.len() >= MAX_ADDRS {
                            break;
                        }
                        addrs.insert(ip);
                    }
                }
                next_lookup = Instant::now() + RELOOKUP;
            }
            let want: Vec<IpAddr> = addrs.iter().copied().collect();
            let due = match &sent {
                None => true,
                Some((list, at)) => *list != want || (!want.is_empty() && at.elapsed() >= RESEND),
            };
            if due {
                match engine.backend().apply_site_bypass(&want).await {
                    Ok(()) => sent = Some((want, Instant::now())),
                    Err(e) => tracing::debug!("could not send the skipped-site list: {e}"),
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
    fn looks_up_each_site_and_its_www_name() {
        let sites = vec![
            "https://www.metrobyt-mobile.com/".to_owned(),
            "example.com".to_owned(),
            "not a site".to_owned(),
        ];
        assert_eq!(
            lookup_names(&sites),
            [
                "example.com",
                "metrobyt-mobile.com",
                "www.example.com",
                "www.metrobyt-mobile.com"
            ]
        );
        assert!(lookup_names(&[]).is_empty());
    }
}
