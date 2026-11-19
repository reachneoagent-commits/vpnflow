// Declaring the command list makes every app command require an explicit
// permission in capabilities/, instead of being callable by default.
const COMMANDS: &[&str] = &[
    "app_info",
    "capabilities",
    "snapshot",
    "events",
    "profiles",
    "country_catalog",
    "update_server",
    "server_health",
    "refresh_server_health",
    "quick_connect",
    "settings",
    "set_settings",
    "connect",
    "disconnect",
    "cancel",
    "import_dialog",
    "import_folder_dialog",
    "new_draft",
    "profile_draft",
    "create_profile",
    "update_profile",
    "duplicate_profile",
    "delete_profile",
    "export_profile",
    "diagnostics",
    "export_diagnostics",
    "speed_test",
    "stop_other_vpns",
    "open_link",
    "scan_downloads",
    "current_network",
    "helper_status",
    "install_helper",
];

/// Embeds the SHA-256 of the staged macOS helper binaries (empty if not staged)
/// so the app can tell whether the installed helper matches this build.
fn embed_helper_hashes() {
    use sha2::{Digest, Sha256};
    for (file, var) in [
        ("vpnflow-daemon", "VPNFLOW_DAEMON_SHA256"),
        ("wireguard-go", "VPNFLOW_WG_SHA256"),
    ] {
        let path = std::path::Path::new("helper").join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let hash = std::fs::read(&path)
            .map(|b| {
                Sha256::digest(b)
                    .iter()
                    .map(|x| format!("{x:02x}"))
                    .collect()
            })
            .unwrap_or_else(|_| String::new());
        println!("cargo:rustc-env={var}={hash}");
    }
}

fn main() {
    embed_helper_hashes();
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
