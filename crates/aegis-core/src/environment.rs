//! Things on this computer that can quietly undermine the VPN: another VPN
//! running at the same time, and browsers that use their own DNS service.
//! Everything here only reads; nothing is changed or sent anywhere.

use std::path::{Path, PathBuf};

/// Other VPN and tunnel apps that fight over routes and DNS. (substring of the
/// process name, lowercase) -> name to show.
const OTHER_VPNS: &[(&str, &str)] = &[
    ("cloudflarewarp", "Cloudflare WARP"),
    ("warp-svc", "Cloudflare WARP"),
    ("nordvpn", "NordVPN"),
    ("expressvpn", "ExpressVPN"),
    ("surfshark", "Surfshark"),
    ("mullvad", "Mullvad"),
    ("protonvpn", "Proton VPN"),
    ("privateinternetaccess", "Private Internet Access"),
    ("openvpn", "OpenVPN"),
    ("windscribe", "Windscribe"),
    ("cyberghost", "CyberGhost"),
];

/// Names of other VPN apps found in a list of process names (one per line).
pub fn match_other_vpns(processes: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for line in processes.lines() {
        let base = line
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(line)
            .to_lowercase();
        for (needle, name) in OTHER_VPNS {
            if base.contains(needle) && !found.contains(name) {
                found.push(name);
            }
        }
    }
    found
}

fn process_list() -> Option<String> {
    #[cfg(unix)]
    let out = std::process::Command::new("ps")
        .args(["-axo", "comm="])
        .output()
        .ok()?;
    #[cfg(windows)]
    let out = std::process::Command::new("tasklist")
        .args(["/fo", "csv", "/nh"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // tasklist csv starts each line with "name.exe"; strip the quotes.
    Some(text.replace('"', ""))
}

/// Cloudflare WARP keeps a background service running even when its window is
/// closed, so "running" alone says nothing. Ask it whether it is connected.
/// `None` when its command-line tool is missing or unreadable.
fn warp_cli() -> Option<PathBuf> {
    [
        "/usr/local/bin/warp-cli",
        "/opt/homebrew/bin/warp-cli",
        "/usr/bin/warp-cli",
        r"C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

/// True when the status text from `warp-cli status` says it is connected.
pub fn warp_status_connected(status: &str) -> bool {
    status.contains("Status update: Connected")
}

fn warp_connected() -> Option<bool> {
    let out = std::process::Command::new(warp_cli()?)
        .arg("status")
        .output()
        .ok()?;
    Some(warp_status_connected(&String::from_utf8_lossy(&out.stdout)))
}

/// Other VPN apps currently running, or `None` if the process list could not be
/// read. A WARP service that is installed but disconnected is not counted.
pub fn other_vpns_running() -> Option<Vec<&'static str>> {
    let mut v = process_list().map(|p| match_other_vpns(&p))?;
    if v.contains(&"Cloudflare WARP") && warp_connected() == Some(false) {
        v.retain(|n| *n != "Cloudflare WARP");
    }
    Some(v)
}

/// What happened when we asked one app to stop.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StopResult {
    pub name: &'static str,
    pub ok: bool,
    pub message: String,
}

fn run_ok(cmd: &mut std::process::Command) -> Result<(), String> {
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Asks each running VPN app to stop, politely. WARP is disconnected with its
/// own tool; others are asked to quit like a normal Quit (never force-killed).
/// Only called when the user presses the button.
pub fn stop_other_vpns() -> Vec<StopResult> {
    let mut out = Vec::new();
    for name in other_vpns_running().unwrap_or_default() {
        let res = if name == "Cloudflare WARP" {
            match warp_cli() {
                Some(cli) => run_ok(std::process::Command::new(cli).arg("disconnect")),
                None => Err("its command-line tool was not found".into()),
            }
        } else if name == "OpenVPN" {
            Err("OpenVPN has no app to quit; stop it where you started it".into())
        } else {
            quit_app(name)
        };
        out.push(match res {
            Ok(()) => StopResult {
                name,
                ok: true,
                message: if name == "Cloudflare WARP" {
                    "disconnected".into()
                } else {
                    "asked to quit".into()
                },
            },
            Err(m) => StopResult {
                name,
                ok: false,
                message: m,
            },
        });
    }
    out
}

#[cfg(target_os = "macos")]
fn quit_app(name: &str) -> Result<(), String> {
    run_ok(
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!("tell application \"{name}\" to quit")),
    )
}

#[cfg(windows)]
fn quit_app(name: &str) -> Result<(), String> {
    // Without /F this is the normal close request, not a force-kill.
    let exe = name.replace(' ', "");
    run_ok(std::process::Command::new("taskkill").args(["/IM", &format!("{exe}.exe")]))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn quit_app(_name: &str) -> Result<(), String> {
    Err("quit it from its own menu".into())
}

/// How a browser resolves names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserDns {
    /// Uses the computer's DNS (so the VPN's choice applies).
    System,
    /// Upgrades to encrypted DNS only when the computer's DNS service supports it.
    Automatic,
    /// Always uses its own DNS service, ignoring the VPN's choice.
    OwnService,
    /// Setting not found; the browser's built-in default applies.
    Unknown,
}

/// Chrome and Edge keep `dns_over_https.mode` in the `Local State` file.
pub fn parse_chromium_dns(local_state: &str) -> BrowserDns {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(local_state) else {
        return BrowserDns::Unknown;
    };
    match v["dns_over_https"]["mode"].as_str() {
        Some("secure") => BrowserDns::OwnService,
        Some("automatic") => BrowserDns::Automatic,
        Some("off") => BrowserDns::System,
        _ => BrowserDns::Unknown,
    }
}

/// Firefox keeps `network.trr.mode` in `prefs.js`: 0 off, 2 and 3 use its own DNS.
pub fn parse_firefox_dns(prefs: &str) -> BrowserDns {
    for line in prefs.lines() {
        let Some(rest) = line.trim().strip_prefix("user_pref(\"network.trr.mode\",") else {
            continue;
        };
        let n: String = rest.chars().filter(char::is_ascii_digit).collect();
        return match n.as_str() {
            "0" | "5" => BrowserDns::System,
            "2" | "3" => BrowserDns::OwnService,
            "1" => BrowserDns::Automatic,
            _ => BrowserDns::Unknown,
        };
    }
    BrowserDns::Unknown
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn chromium_local_state_paths() -> Vec<(&'static str, PathBuf)> {
    let mut v = Vec::new();
    if let Some(h) = home() {
        v.push((
            "Chrome",
            h.join("Library/Application Support/Google/Chrome/Local State"),
        ));
        v.push((
            "Edge",
            h.join("Library/Application Support/Microsoft Edge/Local State"),
        ));
        v.push(("Chrome", h.join(".config/google-chrome/Local State")));
        v.push(("Edge", h.join(".config/microsoft-edge/Local State")));
        v.push((
            "Brave",
            h.join("Library/Application Support/BraveSoftware/Brave-Browser/Local State"),
        ));
        v.push((
            "Arc",
            h.join("Library/Application Support/Arc/User Data/Local State"),
        ));
        v.push((
            "Vivaldi",
            h.join("Library/Application Support/Vivaldi/Local State"),
        ));
        v.push((
            "Brave",
            h.join(".config/BraveSoftware/Brave-Browser/Local State"),
        ));
        v.push(("Vivaldi", h.join(".config/vivaldi/Local State")));
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        v.push(("Chrome", l.join("Google/Chrome/User Data/Local State")));
        v.push(("Edge", l.join("Microsoft/Edge/User Data/Local State")));
        v.push((
            "Brave",
            l.join("BraveSoftware/Brave-Browser/User Data/Local State"),
        ));
        v.push(("Vivaldi", l.join("Vivaldi/User Data/Local State")));
    }
    v
}

fn firefox_profile_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(h) = home() {
        roots.push(h.join("Library/Application Support/Firefox/Profiles"));
        roots.push(h.join(".mozilla/firefox"));
    }
    if let Some(a) = std::env::var_os("APPDATA").map(PathBuf::from) {
        roots.push(a.join("Mozilla/Firefox/Profiles"));
    }
    roots
        .into_iter()
        .filter_map(|r| std::fs::read_dir(r).ok())
        .flat_map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())))
        .collect()
}

/// What each installed browser does with DNS: (browser, setting).
pub fn browser_dns() -> Vec<(&'static str, BrowserDns)> {
    let mut out: Vec<(&'static str, BrowserDns)> = Vec::new();
    for (name, path) in chromium_local_state_paths() {
        if let Ok(text) = std::fs::read_to_string(&path)
            && !out.iter().any(|(n, _)| *n == name)
        {
            out.push((name, parse_chromium_dns(&text)));
        }
    }
    let ff = firefox_profile_dirs()
        .iter()
        .filter_map(|d| read_prefs(d))
        .map(|t| parse_firefox_dns(&t))
        .find(|s| *s != BrowserDns::Unknown)
        .or_else(|| {
            firefox_profile_dirs()
                .iter()
                .any(|d| d.join("prefs.js").exists())
                .then_some(BrowserDns::Unknown)
        });
    if let Some(s) = ff {
        out.push(("Firefox", s));
    }
    out
}

fn read_prefs(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("prefs.js")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_other_vpns_by_process_name() {
        let ps = "/sbin/launchd\n/Applications/Cloudflare WARP.app/Contents/Resources/CloudflareWARP\n\
                  /usr/bin/openvpn\n/usr/sbin/cron\nNordVPN.exe\n";
        assert_eq!(
            match_other_vpns(ps),
            vec!["Cloudflare WARP", "OpenVPN", "NordVPN"]
        );
        assert!(match_other_vpns("/sbin/launchd\n/usr/bin/ssh\n").is_empty());
    }

    #[test]
    fn reads_warp_status() {
        assert!(warp_status_connected("Status update: Connected\n"));
        assert!(!warp_status_connected(
            "Status update: Disconnected\nReason: Settings Changed\n"
        ));
    }

    #[test]
    fn reads_chromium_secure_dns_mode() {
        assert_eq!(
            parse_chromium_dns(r#"{"dns_over_https":{"mode":"secure","templates":"x"}}"#),
            BrowserDns::OwnService
        );
        assert_eq!(
            parse_chromium_dns(r#"{"dns_over_https":{"mode":"automatic"}}"#),
            BrowserDns::Automatic
        );
        assert_eq!(
            parse_chromium_dns(r#"{"dns_over_https":{"mode":"off"}}"#),
            BrowserDns::System
        );
        assert_eq!(parse_chromium_dns("{}"), BrowserDns::Unknown);
        assert_eq!(parse_chromium_dns("not json"), BrowserDns::Unknown);
    }

    #[test]
    fn reads_firefox_trr_mode() {
        let p = |n: u8| format!("user_pref(\"a.b\", 1);\nuser_pref(\"network.trr.mode\", {n});\n");
        assert_eq!(parse_firefox_dns(&p(3)), BrowserDns::OwnService);
        assert_eq!(parse_firefox_dns(&p(2)), BrowserDns::OwnService);
        assert_eq!(parse_firefox_dns(&p(0)), BrowserDns::System);
        assert_eq!(
            parse_firefox_dns("user_pref(\"x\", 1);"),
            BrowserDns::Unknown
        );
    }
}
