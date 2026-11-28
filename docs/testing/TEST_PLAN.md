# Test plan

## Automated (every change)
`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`,
`pnpm lint && pnpm typecheck && pnpm test`, cargo-deny, cargo audit,
pnpm audit (CI). Run Gitleaks only when explicitly requested.

## Live Linux, non-disruptive (developer host)
1. `sudo scripts/test-netns-peer.sh up` (or `pkexec <abs path> up` without a terminal; isolated namespace, split route 10.99.0.0/24)
2. `cargo run -p aegis-linux --example nm_live -- /run/aegis-test/aegis-netns-test.conf`
3. Expect PASS for: import, connect, ping 10.99.0.1, server response confirmed,
   disconnect, delete, no leftover interface/route/NM connection
4. `sudo scripts/test-netns-peer.sh down`

## VM only (disruptive): `scripts/vm/run-vm-tests.sh`
Runs as your normal user. QEMU uses KVM when available and software emulation
otherwise. Routing and firewall changes stay inside the guest.
Boots the SHA512-verified Debian 13 cloud image with `-snapshot` under QEMU
user-mode networking, provisions it with cloud-init (NetworkManager,
systemd-resolved, wireguard-tools, dnsmasq, tcpdump), copies a static musl
`vm_suite` and the peer script in, and runs `vm_suite full` and
`vm_suite full4`. Each mode checks:

- connect + server response confirmed
- IPv4 internet route via the tunnel; endpoint route stays outside it
- ping an IPv4 sentinel that exists only behind the tunnel
- `full`: IPv6 route via the tunnel and IPv6 sentinel ping
- `full4`: IPv6 stays off the uplink and the app verifies the block
- DNS protection reported as Verified
- DNS leak: positive control (lookups visible on the uplink before connecting),
  then both resolved and libc lookups answered by the tunnel's DNS (sentinel
  answer) with 0 port-53 packets on the uplink
- reconnect after `nmcli connection down` behind the app's back
- recovery after `systemctl restart NetworkManager`
- after disconnect, IPv4/IPv6 rules, routes and resolved DNS/domains identical
  to the pre-connect baseline; no leftover NM connection

`KEEP_VM=1` leaves the VM running and prints the SSH command.
The extended suite checks the optional Linux helper block. It exercises kernel
status before apply, startup recovery after the tunnel drops, and cleanup.
Still pending: protected country switching,
suspend/resume, reboot with the tunnel active, Wi-Fi/uplink changes.

## Release matrix
Windows 11 x64 · Ubuntu LTS GNOME · Debian stable KDE · Fedora Workstation ×
scenarios 1–20 from the master prompt (§22): fresh install, valid/invalid
import, reachable/unreachable server, disconnect, switch profiles, Wi-Fi
off/on, network change, suspend/resume, UI restart, NM restart, reboot,
full-tunnel, split, DNS/IPv6, kill switch, uninstall, reinstall.

Release criteria: all automated checks green, live Linux test green, matrix
rows run with results recorded, no open critical security issues.

## Locations regression and acceptance

Core tests cover additive migration without credential copies, ISO validation,
server CRUD/reference checks, duplicate names/endpoints, favorite persistence,
peer-evidence-only recent success, destination preflight, failed switching,
concurrent connect/Quick Connect rejection, cancellation during preflight and
activation, disabled/pending registration, Quick Connect preferences/country/
cooldown, and expired/future health samples. Frontend tests cover directory
navigation/search/filtering/grouping/sorting, empty/error/loading states,
import assignment, favorites, delete confirmation, cancellation and active
Dashboard synchronization. Test fixtures use simulated metadata/documentation
endpoints; they are never shipped as configured production servers.

The VM suite now switches between two country-labelled profiles of one
controlled peer, confirms responses and sentinel traffic, switches back and
deletes the destination. It preserves the existing full/full4 route/DNS/IPv6/
reconnect/NM-restart/restoration checks. These labels do not prove geographic
location or independent remote-server deployment. Run on Linux via
`scripts/vm/run-vm-tests.sh`, or dispatch the separate
`.github/workflows/vpn-integration.yml` workflow. Record all failures, including
known NM restart/firewall behavior, without broad expected-failure exemptions.

Remaining live acceptance: two independently operated country servers, invalid
remote credentials/no handshake, failure during switching/authorization/DNS/
firewall cleanup, suspend/resume, uplink changes, crash/restart and reboot.
Windows CI compilation/portable tests do not substitute for an implemented
Windows service, authenticated IPC/DPAPI tests and a Windows 11 real tunnel.
No protected-switch success can be accepted until continuous protection across
transitions and helper failure paths is implemented and tested. Update TEST_RESULTS with environment and actual evidence.

## Release evidence matrix

| Platform | Target desktop | Current evidence | Required before release |
|---|---|---|---|
| Debian 13 | KDE Plasma | NetworkManager VM and earlier desktop testing. The VM uses an SSH test user and a test Polkit rule | Clean package upgrade/removal, native Polkit prompt, real suspend/reboot and physical link transitions |
| Ubuntu LTS | GNOME | Native-hosted CI configuration and build dependencies | Clean install/upgrade/remove, full DNS/IPv6, helper authorization and fail-closed captures |
| Fedora Workstation | GNOME | RPM build configured | Native RPM lifecycle, NetworkManager and firewalld coexistence, DNS/IPv6 and protected loss captures |
| Windows 11 x64 | Native | Portable protocol tests and Windows-target Rust checks | Authenticated service, native install/upgrade/remove, WFP and routing/DNS/IPv6 acceptance |

Each native run records OS release, desktop, network backend, profile family,
installer version and commit. Use controlled peers and an independent guest
console. Capture physical-interface traffic with a positive control before
testing. Repeat after a connection failure, DHCP/interface change, DNS
change, IPv6 route change, country switch, GUI termination, backend restart
and reboot. Compare routes, rules, resolvers, installed services and owned
firewall state to the baseline after authorized disconnect and uninstall.

## Firewall policy identity and package recovery

The VM suite inserts an additional restrictive rule into `inet aegis_vpn`.
The next helper inspection must reject the policy change, withdraw Verified,
and stop the tunnel. Explicit Disconnect must still remove the changed table.
The snapshot must be a root-owned regular file with mode 0600. Missing,
malformed, linked or permissively readable snapshots must fail verification.

For package removal failure testing, use a disposable VM with the package
installed and an independent console. Back up its helper, temporarily replace
it with a stub that exits unsuccessfully, and request package removal.
The package manager must retain the helper and policy. Restore the original
helper and retry removal, then compare routes, firewall state and user data
with the baseline. A same-version reinstall exercises the upgrade hook path.

## Debian desktop package smoke test

Use a disposable Debian VM with no Aegis desktop package installed. Keep a
console available. Record the package hash, `ip -4 route show` and `sudo nft
list ruleset`. Set a disposable marker in the test user's application data.

1. Install the DEB with `sudo apt-get install /path/to/aegis-preview.deb`.
   Check the installed version with `dpkg-query`, the launcher and license
   files, and `ldd /usr/bin/aegis-desktop` for unresolved dependencies.
2. Reinstall the same file with `sudo dpkg -i /path/to/aegis-preview.deb`.
   For upgrade testing, start with the previous signed package and a real
   test profile instead of the marker.
3. Purge with `sudo dpkg --purge aegis-vpn`. Check that the application
   binary and launcher are gone, user data remains, and routes and nftables
   match the saved baseline. Test the separate helper's lifecycle on its own.
