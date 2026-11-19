<p align="center">
  <img src="apps/desktop/src-tauri/icons/128x128.png" width="128" height="128" alt="VPNFlow icon">
</p>

<h1 align="center">VPNFlow</h1>

<p align="center"><b>A free VPN that shows you whether it is really protecting you. Open source, on your own computer.</b></p>

VPNFlow is part of the FlowApps suite. It is a fork of
[Aegis VPN](https://github.com/mikeholp87/aegis-vpn) by mikeholp87, with our own name, look and
defaults. The WireGuard client, leak checks and everything under the hood come from their work; see
[Credits](#credits). WireGuard is a registered trademark of Jason A. Donenfeld.

> **Preview.** Unsigned builds for Mac and Windows. On Windows the tunnel service is not implemented
> yet, so Windows cannot connect. See [Supported platforms](#supported-platforms),
> [docs/ROADMAP.md](docs/ROADMAP.md) and [docs/testing/TEST_RESULTS.md](docs/testing/TEST_RESULTS.md).

## Download and install

**Easiest:** install and update VPNFlow with the
[FlowApps launcher](https://github.com/reachneoagent-commits/FlowApps-Launcher/releases/latest).
Or download an installer yourself:

| Platform | Installer |
|---|---|
| macOS Intel | [DMG](https://github.com/reachneoagent-commits/vpnflow/releases/download/v0.1.0/VPNFlow_0.1.0_x64.dmg) |
| macOS Apple Silicon (M1 or newer) | [DMG](https://github.com/reachneoagent-commits/vpnflow/releases/download/v0.1.0/VPNFlow_0.1.0_aarch64.dmg) |
| Windows x64 | [Installer](https://github.com/reachneoagent-commits/vpnflow/releases/download/v0.1.0/VPNFlow_0.1.0_x64_en-US.msi) (preview, cannot connect yet) |
| Linux | [Build from source](#build-from-source) |

Older versions are on the [Releases page](https://github.com/reachneoagent-commits/vpnflow/releases),
and every release lists SHA-256 checksums to check the files against.

VPNFlow is not signed with a paid Apple or Microsoft certificate, so your computer warns you the
first time you open it. VPNFlow is a client only: you bring a WireGuard configuration from a VPN
provider or from a server you run yourself.

## What it does

- Imports standard WireGuard `.conf` files (file picker or drag and drop),
  rejecting `PostUp`-style scripts and directives it cannot honour
- Creates and edits profiles without ever showing or accepting raw keys in the UI
- Connects through NetworkManager on Linux; tunnels keep running when the app closes
  and are picked up again when it restarts
- Shows what is actually confirmed: tunnel up, server responded, protection
  configured or verified, instead of a single "secure" badge
- Locations directory for your own servers: country/city metadata, search, favorites,
  recent successful connections, Quick Connect and switching through the same engine
- Automatic reconnect with bounded backoff, diagnostics, sanitized export
- Light and dark themes, keyboard navigation, tray menu
- No account or telemetry; optional connectivity checks have a Settings disclosure

VPNFlow is a client only. It does not come with VPN servers: you bring a WireGuard
configuration from a VPN provider or from a server you run yourself.

## Quick start: connect to a VPN provider

Any provider that offers WireGuard configuration files works. Two examples:

- **Proton VPN** (has a free tier): account.protonvpn.com → **Downloads →
  WireGuard configuration** → platform **GNU/Linux** → choose a server, for
  example in Switzerland → download the `.conf`.
- **Mullvad**: mullvad.net → **Account → WireGuard configuration** → generate a
  key → choose a location → download.

Then:

1. **Protect the file.** It contains your private key, and browsers usually save
   it readable by every user on the machine:

   ```sh
   chmod 600 ~/Downloads/<your-file>.conf
   ```

2. **Import it.** In VPNFlow: **Locations → Add server → Import WireGuard
   Configuration**, pick the file, choose the country (and optionally the city),
   save. VPNFlow rejects files with `PreUp`/`PostUp`/`PreDown`/`PostDown` or
   `SaveConfig` lines instead of running them; delete those lines and import again.
3. **Connect.** Select the country and server, click **Connect**, and wait for
   **Server answered**. "Tunnel up" alone only means the interface exists; the
   server has answered once encrypted data comes back.
4. **Switch location** with **Dashboard → Change location**, then pick another
   server and **Switch server**. **Disconnect** returns to your normal connection.

Provider files are usually *full tunnel* (`AllowedIPs = 0.0.0.0/0, ::/0`): all
internet traffic, IPv4 and IPv6, goes through the VPN server.

For your own server, see [Locations setup and limitations](docs/features/LOCATIONS.md),
which also lists what the server operator has to configure.

## DNS on Linux: make sure lookups go through the VPN

Routing your traffic through the VPN is not enough if your DNS lookups (the
names of the sites you visit) still go to your router or ISP. VPNFlow checks this
after connecting and logs either

```
DNS leak protection verified: systemd-resolved sends all lookups to the tunnel's DNS servers …
```

or a warning such as

```
DNS leak protection check failed: /etc/resolv.conf lists DNS servers outside the tunnel, so some lookups can leak
```

Full-tunnel DNS protection relies on **systemd-resolved** being in charge of
`/etc/resolv.conf`. Check:

```sh
resolvectl status | grep 'resolv.conf mode'
```

- `stub`: correct, nothing to do.
- `foreign`, or `resolvectl` is missing: another program owns `/etc/resolv.conf`
  and lookups can bypass the tunnel. Install and enable systemd-resolved, then
  point `/etc/resolv.conf` at its stub:

  ```sh
  sudo apt install systemd-resolved        # Debian/Ubuntu; Fedora ships it enabled
  sudo systemctl enable --now systemd-resolved
  sudo ln -sf ../run/systemd/resolve/stub-resolv.conf /etc/resolv.conf
  ```

  Reconnect the VPN afterwards (or restart the program that owned the file, see
  Tailscale below).

When it works, the tunnel link receives every lookup:

```sh
$ resolvectl query example.com
example.com: 104.20.23.154       -- link: aegis-xxxxxxxx
```

### Using VPNFlow alongside Tailscale

Tailscale and VPNFlow can run at the same time: tailnet traffic keeps its own
route over `tailscale0`, everything else goes through the VPN.

DNS needs one thing. If Tailscale started while systemd-resolved was not
managing `/etc/resolv.conf`, Tailscale writes that file itself (`# resolv.conf(5)
file generated by tailscale`, `nameserver 100.100.100.100`). All lookups then go to
Tailscale's resolver, which forwards them outside the VPN, and VPNFlow reports a DNS
leak. Fix:

```sh
sudo ln -sf ../run/systemd/resolve/stub-resolv.conf /etc/resolv.conf
sudo systemctl restart tailscaled
```

Tailscale then registers only your tailnet domain (`*.ts.net`) with
systemd-resolved, and everything else follows the VPN. Check that both work:

```sh
resolvectl query example.com                    # link: aegis-…
resolvectl query <machine>.<tailnet>.ts.net     # link: tailscale0
```

If the Tailscale admin console has **DNS → Override local DNS** enabled, Tailscale
still claims every lookup. Turn that off, or stop Tailscale from managing DNS while
using the VPN with `sudo tailscale set --accept-dns=false` (tailnet names then stop
resolving until you set it back to `true`).

## Check that it works

- **In the app:** Dashboard shows Connected, **Server answered**, and the
  DNS/IPv6 protection rows as Verified. Activity lists the same checks.
- **From a terminal:**

  ```sh
  nmcli -t -f NAME,DEVICE,STATE connection show --active | grep Aegis   # activated on aegis-xxxxxxxx
  ip route get 1.1.1.1                     # dev aegis-xxxxxxxx (full tunnel)
  resolvectl query example.com             # link: aegis-xxxxxxxx
  ```

- **Exit location:** `curl https://ifconfig.co/country-iso` asks a third-party
  service which country your traffic appears to come from. Geolocation databases
  are approximate, and the country you assign in Locations is your own label,
  not a verified location.

## Troubleshooting

| Symptom | What it means / what to do |
|---|---|
| Tunnel up, server never answers | Wrong or expired key, the provider removed the config, or UDP to the endpoint is blocked. Download a fresh `.conf`. |
| "Reachability check failed: no answer from the tunnel's DNS server within 3 seconds" right after connecting | Some servers answer DNS slowly in the first seconds. Refresh status; if it keeps failing, try another server. |
| "DNS leak protection check failed" | See [DNS on Linux](#dns-on-linux-make-sure-lookups-go-through-the-vpn). |
| Tailnet names stop resolving | Tailscale is not registered with systemd-resolved; restart `tailscaled` after fixing `/etc/resolv.conf`. |
| Import rejected | The file has script lines (`PostUp` …) or unsupported keys (`Table`, `FwMark`). Remove them; the message says which. |
| Backend unavailable | NetworkManager is not running, or this is Windows (not implemented yet). |

## Supported platforms

| Platform | Status |
|---|---|
| Linux with NetworkManager ≥ 1.16 (Ubuntu, Debian, Fedora; GNOME/KDE) | Split-route live test passed on Debian/KDE; full tunnel, DNS and IPv6 leak tests passed in a Debian 13 VM; full tunnel with commercial servers checked by hand on one Debian/KDE desktop alongside Tailscale. Rest of the matrix pending. |
| Linux without NetworkManager | Not supported yet (shown as unavailable) |
| macOS (Intel and Apple Silicon) | Preview, unsigned |
| Windows 11 x64 | Preview installer; the tunnel service is not implemented yet, so it cannot connect |

## Build from source

See [docs/development/LINUX.md](docs/development/LINUX.md) and
[docs/development/WINDOWS.md](docs/development/WINDOWS.md).

```sh
cd apps/desktop && pnpm install && pnpm tauri dev
# UI only, no real tunnels:
AEGIS_BACKEND=mock pnpm tauri dev
```

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

## Limitations

- Linux kill-switch support requires the installed helper/Polkit policy and
  literal IP endpoints. Unsupported requested protection blocks connection.
  Protected switching is refused. Without protection, traffic can use your normal
  connection between tunnels. See [Locations limitations](docs/features/LOCATIONS.md).
- Full-tunnel DNS protection needs systemd-resolved managing `/etc/resolv.conf`
  (see above). Runtime route/resolver checks distinguish Configured, Verified and
  Failed; they do not replace packet-capture leak tests.
- Known NetworkManager issue: restarting NetworkManager while a full tunnel is
  up can leave a firewall rule behind that breaks later tunnels using the same
  address until reboot. Detection is planned (see docs/architecture/DECISIONS.md D11).
- Handshake times are not readable without root on Linux; the app infers a
  server response from received data.
- Not independently security-audited.

## License

MIT OR Apache-2.0. WireGuard is a registered trademark of Jason A. Donenfeld;
this project is not affiliated with the WireGuard project.

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

## Credits

VPNFlow is a derivative work of [Aegis VPN](https://github.com/mikeholp87/aegis-vpn) by mikeholp87,
licensed under the Apache License 2.0. Thank you to its author and contributors. See
[NOTICE](NOTICE) for details. "Aegis VPN" and its author's names are not licensed for use as VPNFlow
branding.
