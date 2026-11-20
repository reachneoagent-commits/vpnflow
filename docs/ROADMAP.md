# VPNFlow roadmap

Ordered by value to a non-technical user. Done items link to their test record in `docs/testing/TEST_RESULTS.md`.

## Done
- Honest live status: kill switch, DNS, IPv6, internet reachability, all from real system state (macOS).
- Live download/upload rate and totals.
- Connection speed panel: download, upload, latency, jitter, on button press only (Cloudflare endpoint).

- 0.1.4 to 0.1.7 (awaiting live check): bulk delete, import survives a bad file, import auto-fix (IPv4-only), live leak test, folder import, Locations and Advanced WireGuard on one page.
- (fix built, awaiting live check) Localhost stays reachable with the kill switch on (macOS firewall now passes loopback explicitly).
- (built, awaiting visual check) Surfshark-style UI: hero with connection check and leak test, tabbed Locations + Advanced WireGuard, one ordered Settings page, light/dark toggle.

## Next
1. (built, awaiting live test) Auto-connect on launch/login (both off by default), and reconnect after sleep or a network change (on by default; Mac and Linux detect wake and network change).
2. (built, awaiting live test) Trusted networks: skip the VPN on home Wi-Fi, always connect on others. Networks are identified by router address because macOS hides Wi-Fi names.
3. (built, seen running on macOS) Menu bar icon: status, one-click connect, quick server switch, Pause VPN submenu.
4. (built; follows the default "on where supported" until the user chooses; wording still to review) Kill switch on by default, with clearer wording.
5. (first-run screens built, awaiting live test) Folder auto-import of Proton configs and a first-run guide.
6. Fastest-server quick connect using measured speed and ping; show both in Locations.
7. (public IP built on macOS via DNS lookup and shown on the Dashboard; country not shown) Public IP and country display on macOS.
8. Real DNS/IPv6 firewall enforcement in the pf rules, not only route checks.
9. (built, awaiting live test) Scheduled connect and disconnect at set times and days.
10. (live-verified on macOS: blocks ad and tracker domains) Custom DNS and ad blocking: pick AdGuard, Cloudflare malware, Quad9 or your own servers (macOS; needs the updated daemon).

## Built for 0.1.12 (awaiting live check)
- Header button (Disconnect / Cancel / Stop blocking) on every screen, including Dashboard and Locations (0.1.13). Buttons also have pressed, focus and disabled looks.
- Menu bar shows read-only details while connected: server and country, public address when known, time connected, kill switch on/off.
- Check for updates: About page and menu bar item; compares with the latest public GitHub release and offers a Download link.
- DNS list: the default entry now says it blocks nothing and who answers.

## Built for 0.1.14 (awaiting live check)
- About page links now open in the browser (they did nothing before).
- Download and install from inside the app: update files are signed with our own key (private key backup in ~/.vpnflow-keys, secret `TAURI_SIGNING_PRIVATE_KEY` on GitHub), the app refuses anything else, then restarts. Needs one manual install of 0.1.14; first real test is 0.1.14 to 0.1.15.

## Next, from the Surfshark and GitHub comparison
- (built, awaiting live test) Pause VPN for 5 min, 30 min or 2 h, then auto-reconnect.
- (built, awaiting live test) Browser add-ons card in Settings: links to Consent-O-Matic (cookie pop-ups) and uBlock Origin.
- Pinned server: covered by the preferred server setting. Self-healing and failover already exist.
- Skipped: rotating IP, per-app split tunneling.
- More DNS presets only where the provider is confirmed.

## Android (in progress)
- (code written, not yet built) Android phone app: Kotlin plugin drives Android's VPN service with the official WireGuard library; built on GitHub (`.github/workflows/android.yml`), nothing installed on the Mac. Not yet built or run anywhere.
- Not done yet: phone-sized layout, a "?" card for the Android kill switch (system setting "Block connections without VPN"), IPv6 check, opening links on Android.
- Then: Android TV / Google TV layout with QR-code config import, Fire TV packaging (same APK, sideloaded).
- iOS: can build on GitHub but cannot be installed without an Apple developer account (declined). Roku: not possible (router-level VPN only).

## Later
- Developer ID signing and notarization: declined for now; people accept the unsigned warning.
- Windows MSI, universal (Apple Silicon + Intel) Mac build.
- Per-app split tunneling (hard on macOS).
- Linux kill-switch verification, SECURITY.md, DESIGN.md.

## Not planned
Obfuscated servers, multi-hop, quantum-resistant tunnels, DAITA: need server-side support that free Proton configs do not provide. (Ad/tracker blocking is done through custom DNS, item 10.)
