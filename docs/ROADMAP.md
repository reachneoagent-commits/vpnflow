# VPNFlow roadmap

Ordered by value to a non-technical user. Done items link to their test record in `docs/testing/TEST_RESULTS.md`.

## Done
- Honest live status: kill switch, DNS, IPv6, internet reachability, all from real system state (macOS).
- Live download/upload rate and totals.
- Connection speed panel: download, upload, latency, jitter, on button press only (Cloudflare endpoint).

## Next
1. Auto-connect on launch/login, and reconnect after sleep or a network change.
2. (built, awaiting live test) Trusted networks: skip the VPN on home Wi-Fi, always connect on others. Networks are identified by router address because macOS hides Wi-Fi names.
3. Menu bar icon: status, one-click connect, quick server switch.
4. Kill switch on by default, with clearer wording.
5. (first-run screens built, awaiting live test) Folder auto-import of Proton configs and a first-run guide.
6. Fastest-server quick connect using measured speed and ping; show both in Locations.
7. Public IP and country display on macOS (Linux has it).
8. Real DNS/IPv6 firewall enforcement in the pf rules, not only route checks.
9. (built, awaiting live test) Scheduled connect and disconnect at set times and days.
10. (live-verified on macOS: blocks ad and tracker domains) Custom DNS and ad blocking: pick AdGuard, Cloudflare malware, Quad9 or your own servers (macOS; needs the updated daemon).

## Later
- Developer ID signing and notarization (blocks wide distribution).
- Windows MSI, universal (Apple Silicon + Intel) Mac build.
- Per-app split tunneling (hard on macOS).
- Linux kill-switch verification, SECURITY.md, DESIGN.md.

## Not planned
Ad/tracker blocking, obfuscated servers, multi-hop: need server-side support that free Proton configs do not provide.
