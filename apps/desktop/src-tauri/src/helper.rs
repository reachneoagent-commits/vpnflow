//! One-time setup of the macOS background helper (root daemon + wireguard-go).
//!
//! The helper binaries are bundled inside the app (`Resources/helper/`). Their
//! SHA-256 is embedded at build time. Installing asks macOS for the admin
//! password once, copies them to `/usr/local/libexec/vpnflow`, checks the
//! copies against the embedded hashes (so a swapped file is never started as
//! root), then loads the LaunchDaemon. Nothing else is changed.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HelperStatus {
    /// Not macOS, or the platform does not use this helper.
    NotNeeded,
    /// A development build without bundled helper binaries.
    NotBundled,
    /// Installed and identical to the bundled copy.
    Ok,
    /// Not installed yet.
    Missing,
    /// Installed, but from a different build.
    Outdated,
}

const DAEMON_SHA256: &str = env!("VPNFLOW_DAEMON_SHA256");
const WG_SHA256: &str = env!("VPNFLOW_WG_SHA256");

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Pure decision, kept separate so it can be tested on any platform.
pub fn decide(macos: bool, expected: &str, installed: Option<&str>) -> HelperStatus {
    if !macos {
        return HelperStatus::NotNeeded;
    }
    if !is_hex64(expected) {
        return HelperStatus::NotBundled;
    }
    match installed {
        None => HelperStatus::Missing,
        Some(h) if h.eq_ignore_ascii_case(expected) => HelperStatus::Ok,
        Some(_) => HelperStatus::Outdated,
    }
}

#[cfg(target_os = "macos")]
const LIBEXEC: &str = "/usr/local/libexec/vpnflow";

#[cfg(target_os = "macos")]
fn sha256_file(path: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).ok()?;
    Some(
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn current_status() -> HelperStatus {
    #[cfg(target_os = "macos")]
    {
        let daemon = sha256_file(&format!("{LIBEXEC}/vpnflow-daemon"));
        let wg = sha256_file(&format!("{LIBEXEC}/wireguard-go"));
        // Both files must match; report the worse of the two.
        let a = decide(true, DAEMON_SHA256, daemon.as_deref());
        let b = decide(true, WG_SHA256, wg.as_deref());
        if a == HelperStatus::Ok { b } else { a }
    }
    #[cfg(not(target_os = "macos"))]
    {
        decide(false, DAEMON_SHA256, None)
    }
}

pub async fn status() -> HelperStatus {
    tokio::task::spawn_blocking(current_status)
        .await
        .unwrap_or(HelperStatus::Missing)
}

/// Single-quote a string for `sh`.
#[cfg(any(target_os = "macos", test))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The root script. Only hashes embedded at build time are trusted: the copies
/// are verified after installing and removed on a mismatch.
#[cfg(any(target_os = "macos", test))]
fn install_script(src_dir: &str, daemon_sha: &str, wg_sha: &str, plist: &str) -> String {
    let src = sh_quote(src_dir);
    let plist_q = sh_quote(plist);
    format!(
        r#"set -e
D=/usr/local/libexec/vpnflow
L=com.vpnflow.daemon
P=/Library/LaunchDaemons/$L.plist
launchctl bootout system/$L 2>/dev/null || true
install -d -o root -g wheel -m 0755 /usr/local/libexec "$D"
install -o root -g wheel -m 0755 {src}/vpnflow-daemon "$D/vpnflow-daemon"
install -o root -g wheel -m 0755 {src}/wireguard-go "$D/wireguard-go"
bad() {{ rm -f "$D/vpnflow-daemon" "$D/wireguard-go"; echo "helper files did not match this build" >&2; exit 1; }}
[ "$(shasum -a 256 "$D/vpnflow-daemon" | cut -d' ' -f1)" = {daemon_sha} ] || bad
[ "$(shasum -a 256 "$D/wireguard-go" | cut -d' ' -f1)" = {wg_sha} ] || bad
install -d -o root -g wheel -m 0700 /var/db/vpnflow
printf '%s' {plist_q} > "$P"
chown root:wheel "$P"
chmod 0644 "$P"
plutil -lint "$P" >/dev/null
launchctl bootstrap system "$P"
sleep 1
[ -S /var/run/vpnflow.sock ] || {{ echo "the helper did not start" >&2; exit 1; }}
"#
    )
}

#[cfg(target_os = "macos")]
fn run_install(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if !is_hex64(DAEMON_SHA256) || !is_hex64(WG_SHA256) {
        return Err("This copy of VPNFlow was built without the background helper.".into());
    }
    let dir = app
        .path()
        .resource_dir()
        .map_err(|_| "Could not find the helper inside the app.".to_string())?
        .join("helper");
    let dir = dir
        .to_str()
        .ok_or_else(|| "The app is in a folder with an unusual name.".to_string())?;
    // Refuse early if the bundled files are not the ones this build expects.
    for (file, want) in [
        ("vpnflow-daemon", DAEMON_SHA256),
        ("wireguard-go", WG_SHA256),
    ] {
        match sha256_file(&format!("{dir}/{file}")) {
            Some(h) if h.eq_ignore_ascii_case(want) => {}
            _ => return Err("The helper inside the app is damaged. Reinstall VPNFlow.".into()),
        }
    }
    let script = install_script(
        dir,
        DAEMON_SHA256,
        WG_SHA256,
        include_str!("../../../../packaging/macos/com.vpnflow.daemon.plist"),
    );
    let out = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "do shell script (item 1 of argv) with administrator privileges",
            "-e",
            "end run",
            "--",
            &script,
        ])
        .output()
        .map_err(|_| "Could not ask macOS for permission.".to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("-128") || err.to_lowercase().contains("user canceled") {
        Err("Setup was cancelled. Nothing was changed.".into())
    } else {
        tracing::warn!("helper install failed: {}", err.trim());
        Err("Setup did not finish. Nothing unsafe was left behind; try again, or see the log at /var/log/vpnflow-daemon.log.".into())
    }
}

/// Runs only when the user presses "Set up". Asks macOS for the admin
/// password, installs the helper, then restarts the app so it can use it.
pub async fn install(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        tokio::task::spawn_blocking(move || run_install(&handle))
            .await
            .map_err(|_| "Setup stopped unexpectedly.".to_string())??;
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            app.restart();
        });
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("This computer does not need the macOS helper.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const O: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn status_rules() {
        assert_eq!(decide(false, H, None), HelperStatus::NotNeeded);
        assert_eq!(decide(true, "", None), HelperStatus::NotBundled);
        assert_eq!(decide(true, H, None), HelperStatus::Missing);
        assert_eq!(decide(true, H, Some(H)), HelperStatus::Ok);
        assert_eq!(decide(true, H, Some(O)), HelperStatus::Outdated);
    }

    #[test]
    fn script_verifies_hashes_and_quotes_paths() {
        let s = install_script("/Applications/My App's/Resources/helper", H, O, "<plist/>");
        assert!(s.contains(&format!("= {H} ] || bad")));
        assert!(s.contains(&format!("= {O} ] || bad")));
        assert!(s.contains("'/Applications/My App'\\''s/Resources/helper'/vpnflow-daemon"));
        // Hash check happens before the daemon is loaded.
        assert!(s.find("|| bad").unwrap() < s.find("launchctl bootstrap").unwrap());
    }
}
