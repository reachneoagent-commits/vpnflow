<p align="left">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/header-dark.png">
    <img src="assets/header-light.png" width="600" height="160" alt="vpnflow logo and name">
  </picture>
</p>

<p align="center"><b>A free VPN client that shows you whether it is really protecting you. Open source, on your own computer.</b></p>

> VPNFlow is a fork of [Aegis VPN](https://github.com/mikeholp87/aegis-vpn) by mikeholp87, with our own name, look and defaults. Thank you to its author and contributors for the WireGuard client, the leak checks and everything under the hood. See [NOTICE](NOTICE) for details. WireGuard is a registered trademark of Jason A. Donenfeld.

> **Preview.** Unsigned builds for Mac and Windows. On Windows the tunnel service is not implemented yet, so Windows cannot connect. See [Supported platforms](#supported-platforms), [docs/ROADMAP.md](docs/ROADMAP.md) and [docs/testing/TEST_RESULTS.md](docs/testing/TEST_RESULTS.md).

VPNFlow connects to a WireGuard server and then checks that the tunnel, the DNS lookups and IPv6 really go through it. VPNFlow is a client only: it does not come with VPN servers. You bring a WireGuard configuration from a VPN provider or from a server you run yourself.

## Why VPNFlow?

- **Free**: no subscription, no account
- **Honest**: shows what is actually confirmed (tunnel up, server answered, DNS and IPv6 protection verified) instead of a single "secure" badge
- **Private**: no account and no telemetry; the optional connectivity checks are explained in Settings
- **Yours**: bring any WireGuard `.conf` file; your keys never leave your computer

## Features

- Imports standard WireGuard `.conf` files (file picker, drag and drop, or a whole folder), rejecting `PostUp`-style scripts and directives it cannot honour
- Creates and edits profiles without ever showing or accepting raw keys in the UI
- Locations directory for your own servers: country and city, search, favorites, recent connections, Quick Connect and switching
- Automatic reconnect with bounded backoff, diagnostics and a sanitized export
- Connects through NetworkManager on Linux; tunnels keep running when the app closes and are picked up again when it restarts
- Light and dark themes, keyboard navigation, tray menu

## Download and install

**Easiest:** install and update it with the [FlowApps launcher](https://github.com/reachneoagent-commits/FlowApps-Launcher/releases/latest), one app that also manages the other FlowApps on Mac and Windows. Or download an installer yourself below.

Download the installer for your system (latest version, 0.1.15).

| Platform                          | Installer |
| --------------------------------- | --------- |
| macOS Apple Silicon (M1 or newer) | [DMG](https://github.com/reachneoagent-commits/vpnflow/releases/download/v0.1.15/VPNFlow_0.1.15_aarch64.dmg) |
| macOS Intel                       | [DMG](https://github.com/reachneoagent-commits/vpnflow/releases/download/v0.1.15/VPNFlow_0.1.15_x64.dmg) |
| Windows x64                       | [Installer](https://github.com/reachneoagent-commits/vpnflow/releases/download/v0.1.15/VPNFlow_0.1.15_x64_en-US.msi) (preview, cannot connect yet) |
| Android phone, Google TV, Fire TV | [APK](https://github.com/reachneoagent-commits/vpnflow/releases/download/android-preview-1/vpnflow-android-aarch64.apk) (untested preview; [how to install](#android-google-tv-and-fire-tv)) |
| Linux                             | [Build from source](#build-it-yourself) |

Older versions are on the [Releases page](https://github.com/reachneoagent-commits/vpnflow/releases). Every release lists SHA-256 checksums so you can check the files.

VPNFlow isn't signed with a paid Apple or Microsoft certificate (that costs money every year, and VPNFlow is free). Your computer will warn you the first time you open it. The warning is expected, and the steps below get past it once.

### Android, Google TV and Fire TV

An untested preview. The screens are made for a phone, not a TV remote yet.

1. Download the `.apk` from the table above. On a Fire TV, install the free **Downloader** app, open it and type the APK link.
2. Allow installing from this source when asked (Fire TV: Settings, My Fire TV, Developer options, Install unknown apps).
3. Open VPNFlow. On Fire TV it is under **Your apps & channels**.

Preview builds use a temporary signing key, so uninstall the old one before installing a newer one.

### Mac

1. Download the `.dmg` that matches your Mac:
   - **`aarch64`**: Macs with an M1, M2, M3 or newer chip.
   - **`x64`**: older Macs with an Intel chip.
2. Open the `.dmg` and drag **VPNFlow** into **Applications**.
3. Open VPNFlow. macOS will say it can't verify the app. Click **Done** (not Move to Trash).
4. Open **System Settings → Privacy & Security**, scroll down to the message about VPNFlow, click **Open Anyway**, and enter your password.

If step 4 doesn't show Open Anyway, run this once and open VPNFlow again:

```bash
xattr -cr /Applications/VPNFlow.app
```

To check a download against its checksum, run this in the folder with the file:

```bash
shasum -a 256 VPNFlow_0.1.1_aarch64.dmg
```

### Windows

1. Download the `.msi` installer and run it.
2. If Windows says **"Windows protected your PC"**, click **More info**, then **Run anyway**.
3. Finish the installer and open VPNFlow. In this preview the tunnel service is not implemented yet, so Windows cannot connect.

To check the download, run this in PowerShell in the folder with the file:

```powershell
Get-FileHash .\VPNFlow_0.1.1_x64_en-US.msi -Algorithm SHA256
```

### Linux

There is no installer yet. Build it from source (below). Linux needs NetworkManager 1.16 or newer.

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

## Build it yourself

See [docs/development/LINUX.md](docs/development/LINUX.md) and
[docs/development/WINDOWS.md](docs/development/WINDOWS.md).

```sh
cd apps/desktop && pnpm install && pnpm tauri dev
# UI only, no real tunnels:
AEGIS_BACKEND=mock pnpm tauri dev
```

Linux developer notes (dev icon, helper install, preview packaging) are in [docs/development/LINUX_EXTRAS.md](docs/development/LINUX_EXTRAS.md).

## About this fork

VPNFlow's WireGuard client, leak checks and Linux backend come from Aegis VPN. The upstream project is at <https://github.com/mikeholp87/aegis-vpn>.

### Credits

- **[Aegis VPN](https://github.com/mikeholp87/aegis-vpn)** by mikeholp87: the WireGuard client, leak checks and backends (Apache 2.0).
- **[WireGuard](https://www.wireguard.com/)**: the VPN protocol. WireGuard is a registered trademark of Jason A. Donenfeld. VPNFlow is not affiliated with the WireGuard project.

## License

MIT OR Apache-2.0. VPNFlow is a derivative work of Aegis VPN (Apache License 2.0). "Aegis VPN" and its author's names are not licensed for use as VPNFlow branding.
