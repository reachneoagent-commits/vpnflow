const COMMANDS: &[&str] = &[
    "prepare",
    "connect",
    "disconnect",
    "status",
    "open_vpn_settings",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
