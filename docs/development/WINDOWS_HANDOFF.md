# Windows build handoff

Written 2026-11-17 at the end of the Linux work. Read this, then
`docs/architecture/DECISIONS.md` D16, `docs/ROADMAP.md` Phase D and
`docs/security/THREAT_MODEL.md`. Rules: `AGENTS.md` (they apply on Windows too).

## Goal

VPNFlow establishes a real, protected WireGuard tunnel to the user's own
servers on Windows 11, with the same honest-state behaviour as Linux. Reuse the
official WireGuard implementation (embeddable-dll-service); never write
crypto or a tunnel.

## What exists

| Piece | State |
|---|---|
| `crates/aegis-windows/src/proto.rs` | Pipe protocol (`Request`/`Response`, 4-byte LE length + JSON frame, `MAX_FRAME`, `SecretText` with redacted `Debug`). Portable, 6 tests pass on Linux. |
| `crates/aegis-windows/src/names.rs` | Tunnel name `aegis-` + 24 hex of the profile id (WireGuard limit 32), service `WireGuardTunnel$<name>`, ownership checks. 3 tests pass on Linux. |
| `crates/aegis-windows/src/lib.rs` | `ServiceBackend`: pipe client implementing every `VpnBackend` method. **Compiled for `x86_64-pc-windows-msvc` from Linux, never run.** `protection()` returns `Off` on purpose. |
| `apps/desktop/src-tauri/src/backend.rs` | Already selects `ServiceBackend::connect()` on Windows, else `Unavailable(reason)`. Never falls back to the mock. |
| `services/windows-service/` | Empty (`src/` only). Not in the workspace (`Cargo.toml` has it commented out). |

Nothing in this list has run on Windows. The Linux side is done and VM-tested;
use it as the model: `crates/aegis-linux`, `services/linux-helper`, D12 to D15.

## Step 0: first hour on the Windows machine

1. Install Rust (MSVC toolchain), Node + pnpm, Visual Studio Build Tools (C++), WebView2.
2. `cargo test -p aegis-windows` should show 9 passed.
3. `cargo clippy -p aegis-windows --all-targets -- -D warnings`.
4. Try `cargo check -p aegis-desktop`. A cross-check from Linux failed in the
   Tauri build script (not investigated). Fix whatever fails natively.
5. `cd apps/desktop && pnpm install && pnpm lint && pnpm typecheck && pnpm test`
   (28 tests). Then `AEGIS_BACKEND=mock pnpm tauri dev` to confirm the UI
   works at all (the banner must say simulated), then without the variable to
   see the honest "service is not running" state.
6. Record what ran, including failures, in `docs/testing/TEST_RESULTS.md`.

## Step 1: the service (`services/windows-service`)

A LocalSystem Windows service. The only privileged Windows code, like
`services/linux-helper`.

- Enable it in the workspace `Cargo.toml`. Candidate crates for the service
  plumbing: `windows-service`, `windows-sys` / `windows`. Check maintenance and
  RustSec before adding any (AGENTS.md convention). Registry already has
  `windows` 0.62 cached on the Linux machine.
- **Pipe server** at `proto::PIPE_NAME`:
  - Create with an explicit security descriptor (DACL). Grant only interactive
    logged-on users (and SYSTEM/Administrators). Never Everyone. Reject remote
    clients (`PIPE_REJECT_REMOTE_CLIENTS`). Keep one free instance listening.
  - Verify the client on each connection (impersonate and check the SID or
    session) before acting.
  - Handle `Hello{version}`: reply `Ok` or `Error{Unsupported}` on mismatch.
  - Bound everything (`proto::read_frame` already caps frame size). Serialize
    mutating requests with one lock, like `Engine`'s op lock.
- **Requests to implement** (see `proto::Request`): `Capabilities`, `Install`,
  `Uninstall`, `Load`, `Installed`, `Activate`, `Deactivate`, `Status`,
  `ApplyKillSwitch`, `RemoveKillSwitch`, `KillSwitchActive`.
  - `Capabilities`: `backend: BackendKind::WindowsService`, `simulated: false`,
    `Support::Unsupported(reason)` for anything not implemented yet, so the UI
    stays honest. `available` is `Supported` only when tunnels can really be brought up.
  - `Install`: parse with `aegis_config::parse` and re-validate
    (`aegis_config::validate`) in the service. Never trust the app's check.
  - `Status`: `TunnelStatus` with `interface`, rx/tx bytes and
    `last_handshake_ms` (see below). `Activate` resolves only once the link is up.
- **Config storage:** `%ProgramData%\VPNFlow\`, DPAPI-protected (machine
  scope, because the service is LocalSystem), ACL SYSTEM + Administrators only.
- **Tunnel service:** per D16, install `WireGuardTunnel$aegis-<24 hex>` through
  the SCM exactly as the embeddable-dll-service README says:
  `SERVICE_WIN32_OWN_PROCESS`, auto start, error control normal, dependencies
  `Nsi` and `TcpIp`, **`SERVICE_SID_TYPE_UNRESTRICTED` (essential)**, command
  line `<our tunnel exe> /service <conf>`. That exe loads `tunnel.dll` by
  **absolute path** from the install directory (call `SetDefaultDllDirectories`
  first; THREAT_MODEL lists DLL search-path attacks) and calls
  `WireGuardTunnelService(LPCWSTR conf_file)`. `wireguard.dll` must sit beside it.
- **Ownership:** act only on services/adapters for which
  `names::is_owned_service_name` / `is_owned_tunnel_name` is true (AGENTS.md
  rule 7). `installed()` must list only those.
- **Crash safety:** on service start, find leftover owned tunnels, plaintext
  config files and kill-switch filters from a previous run and clean up or
  adopt them (Linux `recover` is the model). Disconnect must remove the
  tunnel service and any temp config.
- **Never log keys, config text, or file contents.** Log ids and error kinds only.

### Open question to settle first (ANSWERED 2026-11-17 from source, see DECISIONS D16)

Resolved: `tunnel.dll` loads `<name>.conf.dpapi` (DPAPI, description = tunnel
name) as well as plaintext `.conf`, so store the encrypted file and write no
plaintext. Still to confirm by running it on Windows. The text below is the
original question, kept for context.

The embeddable README gives the exported function and service install but does
**not** say whether `tunnel.dll` reads an encrypted config or only a plaintext
`.conf`. D16 assumes plaintext in a SYSTEM-only directory, present only while
the tunnel runs and deleted on stop and at service start. Test what
`tunnel.dll` actually accepts, then update D16 with the answer. Do not ship the
assumption as fact.

## Step 2: handshake and statistics

Linux gets handshake times from `wg show … latest-handshakes` in the root
helper. On Windows, find out what the tunnel service/wireguard-nt exposes
(wireguard-nt has an adapter API returning peer handshake and byte counters).
Fill `TunnelStatus.last_handshake_ms` and set
`exact_handshake_time: Supported` only when it works. Without it the engine
infers a server reply from received bytes (`Capabilities` note).

## Step 3: protection, verified not assumed

`Feature` states are `Off/Unsupported/Configured/Verified/Failed`. Implement
`verify_protection`, `check_reachability` and `check_public_ip` on the Windows
backend (the trait defaults give `Off`). Do not report `Verified` from a
setting; check the machine, as Linux does (route lookups, resolver state).

- **DNS:** which interface owns DNS, whether the name resolver (including
  NRPT and DNS-over-HTTPS settings) sends queries elsewhere, Smart Multi-Homed
  Name Resolution leaking to other adapters. Verify at runtime.
- **IPv6:** IPv4-only full tunnels must not leak IPv6. Linux uses a `::/0`
  device route at metric 1 on the tunnel. Decide the Windows equivalent and
  verify it with a route lookup. Do not rely on `type=unreachable` style
  routes without testing them across a restart (it failed on Linux).
- **Reachability:** tunnel-DNS lookup of example.com through the profile DNS
  (D13, `aegis-linux::verify` has the logic to port).
- The official tunnel service installs its own firewall rules for full
  tunnels. **I did not verify what they block.** Read the source and test it
  before claiming any kill-switch behaviour from it.

## Step 4: kill switch

Contract is in `VpnBackend::apply_kill_switch` docs (atomic, survives app
crash, removed on disconnect, allows tunnel interface, UDP to the server
endpoints, DHCP, IPv6 neighbour discovery, LAN only when `allow_lan`).

- Use the Windows Filtering Platform with an Aegis-owned provider key and
  sublayer, so removal and detection work by key and never touch other
  filters. No `netsh` shelling. Decide dynamic (vanish with the service
  session) versus persistent (survive a service crash); D14 chose "session
  scoped, gone after reboot" for Linux, keep that behaviour unless the user
  decides otherwise.
- IP-literal endpoints only (the engine already refuses hostnames while the
  kill switch is on, `kill_switch_endpoints`).
- `kill_switch_active` must report the real state (filters present), so a
  leftover block is adopted at startup (`Engine::recover`).
- Do NOT test this on a machine you need online. See "Test safety".

## Step 5: sleep/resume and network change

Linux: `crates/aegis-linux/src/events.rs` produces `SystemEvent::Resumed` and
`NetworkChanged` and the engine does the rest (D15, already unit-tested with
injected events). Implement `VpnBackend::system_events()` for Windows
(power broadcast `PBT_APMRESUMEAUTOMATIC` and network-change notifications such
as `NotifyIpInterfaceChange` / `NotifyRouteChange2`), sent from the service or
the app. Return `None` until it works; the engine copes.

## Step 6: packaging

- Build `tunnel.dll` and `wireguard.dll` in CI (the README says `./build.bat` in
  the `tunnel` folder, outputs `amd64/tunnel.dll`; `wireguard.dll` comes from
  the wireguard-nt download). Pin versions and record checksums.
- NSIS installer (`tauri.conf.json` already lists `nsis`): install and start
  the service, uninstall removes the service, tunnels, filters, config
  directory. Uninstall must leave no block behind (the Linux
  `install-helper.sh uninstall` runs `remove` first for the same reason).
- Code signing is a later decision for the user.

## Tests that need a real Windows 11 machine

Tracked in `docs/testing/TEST_PLAN.md`. At minimum, with results written to
`TEST_RESULTS.md`:

1. Real tunnel to a user-owned server: handshake, traffic, public IP changes.
2. Full-tunnel IPv4 routing and the IPv6 non-leak, with a packet capture on
   the physical NIC (the Linux VM suite's tcpdump leak capture with a positive
   control is the model).
3. DNS leak test, including SMHNR and an alternate NIC.
4. Kill switch: LAN blocked/allowed, tunnel drop, service crash, app crash,
   reboot, uninstall.
5. Disconnect cleanup: no leftover service, adapter, route, filter, file.
6. Pipe security: another local user, a remote client, a malformed or
   oversized frame, a fuzzed request.
7. Sleep/resume and Wi-Fi/Ethernet switching on hardware.
8. Tunnel failure handling: bad endpoint, wrong key, server down.

## Test safety

Disruptive tests (full tunnel, kill switch, route changes) only in a Windows VM
or a spare machine you can reach out-of-band. The Linux harness refuses to run
disruptive tests off the test VM (`/etc/aegis-test-vm`); build the equivalent
guard for Windows before writing such tests. Never test against servers that
are not the user's own, and never invent endpoints, keys or results.

## Constraints that must not regress

- No secrets in the webview, logs, diagnostics, events or the repository;
  `SecretKey` has no `Display`/`Serialize`. The pipe protocol is the one place
  config text crosses processes: keep `SecretText`, never log requests wholesale.
- No file paths from web content. No profile scripts (`PreUp` etc. rejected).
- Tauri commands need an `allow-*` entry in `capabilities/default.json`.
- Mock only with `AEGIS_BACKEND=mock`, always labelled.
- Update `docs/ROADMAP.md`, `DECISIONS.md`, `THREAT_MODEL.md` and
  `TEST_RESULTS.md` as you go. Newest `TEST_RESULTS.md` entry goes on top.

## Known gaps carried over from Linux

Hostname endpoints are refused while the kill switch is on; protected profile
switching stays refused; forwarded (container/VM) traffic is not covered by the
Linux kill switch; `Engine::set_settings` returns `Busy` while the monitor holds
the op lock; after a network change the UI shows `NoResponse` but Aegis does not
yet re-activate the tunnel by itself (D15 follow-up). Real suspend/resume, Wi-Fi
roaming and the desktop Polkit prompt were not tested on Linux either.
