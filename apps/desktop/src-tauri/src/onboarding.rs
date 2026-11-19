//! First-run helpers. The web UI never supplies a URL or a path: it names one
//! of a fixed set of links, and Rust scans the user's own Downloads folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

/// Fixed links the onboarding may open in the user's normal browser.
pub fn link(which: &str) -> Option<&'static str> {
    Some(match which {
        "signup" => "https://account.proton.me/signup?plan=free",
        "login" => "https://account.proton.me/login",
        "wireguard" => "https://account.proton.me/u/0/vpn/WireGuard",
        "help" => "https://protonvpn.com/support/wireguard-configurations",
        _ => return None,
    })
}

pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut c = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut c = std::process::Command::new("xdg-open");
    c.arg(url).spawn().map(|_| ())
}

pub fn downloads_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join("Downloads"))
}

const MAX_FILES: usize = 40;
const MAX_BYTES: u64 = 64 * 1024;

static SEEN: Mutex<Option<HashSet<(PathBuf, u64)>>> = Mutex::new(None);

/// `.conf` files in `dir` changed at or after `since_ms` (Unix ms) that this
/// run has not offered before. Each file is offered once.
pub fn new_conf_files(dir: &Path, since_ms: u64) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut guard = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let seen = guard.get_or_insert_with(HashSet::new);
    let mut out = vec![];
    for e in rd.flatten().take(5000) {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("conf") {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() || md.len() > MAX_BYTES {
            continue;
        }
        let ms = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d: Duration| d.as_millis() as u64);
        if ms < since_ms || !seen.insert((p.clone(), ms)) {
            continue;
        }
        out.push(p);
        if out.len() >= MAX_FILES {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_links_open() {
        assert!(
            link("signup")
                .unwrap()
                .starts_with("https://account.proton.me/")
        );
        assert!(link("https://evil.example").is_none());
        assert!(link("").is_none());
    }

    #[test]
    fn finds_new_conf_files_once() {
        let d = std::env::temp_dir().join(format!("vpnflow-onb-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.conf"), "x").unwrap();
        std::fs::write(d.join("b.txt"), "x").unwrap();
        std::fs::write(d.join("big.conf"), vec![b'x'; 70_000]).unwrap();
        assert_eq!(new_conf_files(&d, 0).len(), 1);
        assert!(new_conf_files(&d, 0).is_empty());
        let d2 = d.join("none");
        assert!(new_conf_files(&d2, 0).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn old_files_are_ignored() {
        let d = std::env::temp_dir().join(format!("vpnflow-onb2-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("old.conf"), "x").unwrap();
        assert!(new_conf_files(&d, u64::MAX).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
