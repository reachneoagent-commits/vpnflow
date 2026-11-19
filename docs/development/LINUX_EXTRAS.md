# Linux extras for developers

Moved out of the README. Release notes and packaging for the Linux preview.

## Dev icon and taskbar

The app icon source is `apps/desktop/src-tauri/icons/icon.svg`; regenerate all
sizes with `pnpm tauri icon src-tauri/icons/icon.svg`.

**Taskbar icon in dev builds (KDE/GNOME on Wayland):** the desktop looks up a
window's icon from an installed `.desktop` file named after the app id
(`aegis-desktop`), which `pnpm tauri dev` does not install. To see the icon while
developing:

```sh
for n in 32 64 128 256 512; do
  install -Dm644 /dev/null ~/.local/share/icons/hicolor/${n}x${n}/apps/aegis-desktop.png
  magick -background none apps/desktop/src-tauri/icons/icon.svg -resize ${n}x${n} \
    ~/.local/share/icons/hicolor/${n}x${n}/apps/aegis-desktop.png
done
cat > ~/.local/share/applications/aegis-desktop.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=VPNFlow
Exec=aegis-desktop
Icon=aegis-desktop
StartupWMClass=aegis-desktop
NoDisplay=true
EOF
kbuildsycoca6 2>/dev/null || update-desktop-database ~/.local/share/applications
```

## Linux helper: install or upgrade

Disconnect and quit VPNFlow before replacing or uninstalling the helper. From
the repository root, update the binary and Polkit policy together:

```sh
cargo build --release -p aegis-helper --locked
sudo scripts/install-helper.sh install target/release/aegis-helper
```

Firewall changes affect every local user and require administrator
authentication. Read-only status and handshake queries are password-free for
the active desktop user. Reopen VPNFlow after upgrading; an older helper lacks
the new `status` command, so the app reports unknown firewall state until it
is upgraded. Custom Polkit rules need the separate
`org.aegisvpn.helper.killswitch.apply` and `.remove` action IDs.

`sudo scripts/install-helper.sh uninstall` removes the firewall block first.
If cleanup fails it keeps the helper and policy installed so you can retry.

## Audit and preview packaging

The current release classification is **Development Preview**. Linux uses the
real NetworkManager backend. The DEB and RPM packages include the helper and
matching Polkit policy for protected mode. Protected switching and Always On
remain unavailable. The Windows
adapter stays offline until the authenticated tunnel service is implemented.

For a self-hosted WireGuard endpoint, configure the server peer, UDP listener,
return routes and any forwarding or DNS required by your profile. Then import
the client configuration from Locations and verify the peer response and routed
traffic. See [Locations](docs/features/LOCATIONS.md), [DNS](docs/security/DNS_PROTECTION.md),
[IPv6](docs/security/IPV6_PROTECTION.md), and [recovery](docs/security/RECOVERY.md).

Build the Linux desktop preview with Tauri's DEB or RPM target. Inspect its
dependencies and checksum, then use the matching package manager on a
disposable test system. Disconnect and remove any remaining firewall block
before uninstalling. See the [release checklist](docs/releases/RELEASE_CHECKLIST.md)
and [remediation report](docs/audits/REMEDIATION_REPORT.md).
