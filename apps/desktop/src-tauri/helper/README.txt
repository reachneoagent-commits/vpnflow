macOS only. scripts/macos/stage-helper.sh copies the root daemon (vpnflow-daemon)
and wireguard-go here before `tauri build`. They are bundled into the app and
installed by the in-app "Set up" step. The binaries are not committed.
