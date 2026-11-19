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
    "open_link",
    "scan_downloads",
    "current_network",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
