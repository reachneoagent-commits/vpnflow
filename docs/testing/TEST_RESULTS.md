# Test results

Only what was actually executed. Newest first.

## 2026-11-18 — Main branch integration

Integrated the audit changes with GitHub main at `2854b28`. The integrated
checkout passed workspace formatting and Clippy, 136 Rust tests, the frontend
production build and 30 frontend tests. The Windows adapter and service passed
all-target MSVC-target compile checks.

Built unsigned Linux debug DEB and RPM packages. Both archives include the
helper and matching Polkit policy at their system paths. The packaged removal
hooks matched their sources. Stub checks confirmed that failed firewall cleanup
aborts removal, successful cleanup permits removal, and upgrade leaves the
block in place. ShellCheck and shfmt passed for both hooks and the test scripts.
Native package transactions, desktop prompts and a live Windows tunnel remain
pending.

## 2026-11-18 — Release readiness audit and remediation

Host was Debian forky/sid, Linux 7.3.4+deb14-amd64, Rust 1.99.1, Node
24.16.0 and pnpm 12.21.0. Baseline was main at `548523e` with the hardened
helper and other existing working-tree edits. Baseline workspace formatting,
locked offline check, Clippy and all-target tests passed (131 Rust tests).
Frontend lint, types, 29 tests and production build passed.

| Check | Observed result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo check --workspace --all-targets --locked --offline` | Passed |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Passed |
| `cargo test --workspace --all-targets --locked --offline` | 136 passed in 14 test binaries, including empty example harnesses |
| `cargo check -p aegis-windows --all-targets --target x86_64-pc-windows-msvc --locked --offline` | Passed, source and test compilation only |
| `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm build` | Passed under `apps/desktop`. 30 tests passed. Biome reported the pre-existing preset deprecation |
| `pnpm audit --prod` | No known vulnerabilities found |
| `cargo audit --db "$advisory_db" --no-fetch --no-yanked` | Passed using a fresh RustSec checkout at `233155b` (2026-11-17). Five unmaintained dependency warnings (`chrono`, `proc-macro-error`, `zbus_names`, `zvariant`, `zvariant_derive`). Yanked-version checks were excluded |
| `cargo deny --offline check` | Advisories, bans, licenses and sources passed with a writable scratch copy of the configured database and policy path. The normal invocation initially failed to lock the read-only cache |
| ShellCheck, `shfmt -d`, and `bash scripts/install-helper.test.sh` | Passed for the applicable installer and VM scripts. Package installation on the host was outside the test |
| `./node_modules/.bin/tauri build --debug --ci --no-sign --bundles deb,rpm -- --locked --offline` | Both Linux desktop bundles built. SHA-256 verification, `dpkg-deb` and RPM CPIO listing passed |

The new regressions include external firewall loss, Linux socket binding,
live resolver address matching, activation-scoped handshake caching,
preservation of untracked profiles, profile-edit rollback, local-only Tauri
command grants and Rust-to-UI settings synchronization.

Local artifact hashes:

- DEB `171e5475c7efa92f8202d0d66d77a498fa59b387ac7a118bc6b3afb85725a0be`
- RPM `747e17351366c08dc264843d38f1cef9bcc8958b6f556f66cfa8b0f0a5b0ade6`

Both archives contain the desktop binary, launcher and icons. The separate
helper and Polkit policy are needed for protected mode. Neither package was
installed. The hosted preview workflow was not triggered.

The attempted baseline and current desktop startup measurements under Xvfb
timed out after eight seconds waiting for a visible window. Desktop portal
and runtime socket errors were observed. Startup, idle and connected resource
measurements remain unavailable. Gzip JS artifact size was 90.09 kB at
baseline and 90.14 kB after remediation.

## 2026-11-18 — Live validation of the hardened Linux helper

The validation snapshot passed 131 Rust tests with
`cargo test --workspace --all-targets --locked`. Focused all-target Clippy
passed for the Linux backend and helper.
ShellCheck, shfmt, the installer and policy test, and ShellCheck on the VM CI
setup step passed.

Without `/dev/kvm`, the runner booted a SHA512-verified Debian 13 snapshot using
QEMU software emulation. The first attempt hit a read-only `/var/tmp` when QEMU
created its disk overlay. Storing the overlay in the private run directory let
the guest boot. SSH initially rejected a host configuration file with invalid
ownership. The runner now gives SSH and SCP explicit per-run settings. A
cleanup edit interrupted an active Bash runner during the next attempt, so the
live suite ran from a fixed copy of the final script.

Guest: Debian 13, kernel 6.12.117+deb13-cloud-amd64, NetworkManager 1.52.1.
**221 passed, 0 failed** (110 `full`, 111 `full4`). The runner exited 0.
Results apply to the binaries built at the start of the run.
Both modes used the updated helper and policy at their system paths inside the
VM. The helper reported no block before apply, rediscovered the retained block
from the kernel during startup after a forced tunnel drop, and reported no block
after Disconnect. Packet captures confirmed the uplink block and the positive
control after removal. The route, rule, DNS and nftables snapshots matched the
baseline after cleanup. The guest's test Polkit rule authorized helper actions.
Desktop administrator prompts and Windows runtime remain untested.

## 2026-11-17 — Linux helper security follow-up

Scope: helper inputs/privileged filesystem access, mutation serialization,
Polkit authorization, kernel inspection, core recovery, wrapper propagation
and uninstall failure handling. IPv6 routing and the ruleset's allowed traffic
were not changed. The user explicitly chose administrator authentication for
machine-wide firewall changes; read-only actions remain separately scoped.

- `cargo test --workspace --all-targets --locked`: **131 passed**, 13 suites.
  Helper: **7 passed**, including symlink/hard-link rejection, runtime mode,
  competing lock descriptors, failed readback retaining rules, legacy marker
  links never written, table absence versus command errors, strict argv and
  ruleset ownership. All nft calls in failure tests are stubs; no host firewall
  commands ran.
- Linux status-response parsing checks true/false, malformed replies, explicit
  upgrade guidance for the older helper's usage error and preserved permission
  errors. The helper binary and policy must be upgraded together.
- New core regressions cover failed startup inspection retaining Uncertain,
  failed cleanup remaining retryable, failed apply discovering a leftover
  block, and marker cleanup failing after rules are already removed. The last
  regression failed with a false Verified claim before its fix and passed after.
- Frontend Vitest: **29 passed**; includes Disconnect remaining enabled with
  uncertain state and a null profile. TypeScript and Biome passed (existing
  Biome configuration deprecation notice remains).
- `bash scripts/install-helper.test.sh`: pass. It parses the policy XML to
  check admin-only apply/remove and argv1-scoped read-only permissions, then
  uses temporary dummy helpers to prove failed cleanup prevents uninstall.
- Shellcheck and shfmt passed for the installer, its test and VM runner.
  Workspace formatting and all-target Clippy with warnings denied passed.
- Windows target Clippy for the Windows adapter and helper, and musl target
  check for the helper passed. These are compile checks, not Windows/live tests.

The unprivileged helper `status` invocation exited 2 with the expected root
requirement, before any nft call. Intermediate Clippy checks caught a
collapsible-if warning and test-module placement, fixed before the final
check. Focused Jev Review ran during implementation; its scores are
advisory and do not establish runtime safety.

Attempted `bash scripts/vm/run-vm-tests.sh`: exited 1 before booting because
`/dev/kvm` is absent (runner reports no write access). The updated binary and
policy were not installed on the host; real Polkit prompts and post-hardening
packet-capture tests remain unrun. Existing live VM results below describe
earlier revisions. No Windows runtime, dependency audit or Gitleaks scan was run.
Upgrade instructions: [README](../../README.md#linux-helper-install-or-upgrade).

## 2026-11-17 — deb now packages the kill-switch helper (build and contents only)

Security review finding: the installed `.deb` shipped no helper or Polkit policy, so the
kill switch reported Unsupported. `tauri.conf.json` now installs
`/usr/libexec/aegis-vpn/aegis-helper` and the policy, adds `nftables` and `pkexec` to
Depends and `wireguard-tools` to Recommends, and runs `aegis-helper remove` from
`prerm` on remove/purge (not on upgrade). Run: `cargo build --release -p aegis-helper`,
`pnpm tauri build --bundles deb` from a clean worktree, then `dpkg-deb -c/-f/-e`.
Result: both files present (helper 0755, policy 0644), Depends/Recommends as above,
`prerm` present and executable; `shellcheck` and `shfmt -d` pass on the new scripts.

**Not run:** installing this package, a kill-switch run from the installed helper, the
upgrade/uninstall paths of `prerm`, and the RPM bundle (config keys shared with the deb
parsed, but no `rpm` bundle was built).

## 2026-11-17 — GitHub CI on the pushed commits (run 44034535510, head `15ecb99`)

Passed: Rust fmt/clippy/tests (Linux), frontend lint/typecheck/tests, advisories and
licenses (`cargo-deny`, `npm audit`). The previous run (`548523e`) had failed the
Windows job on the `AnyBackend` `large_enum_variant` clippy error fixed in `a0b3345`;
that step now passes.

**Windows build job failed** at `cargo test --workspace --locked`: 1 failure,
`store::a_directory_someone_else_created_is_refused`. Cause: the runner is an
**elevated administrator**, so a directory the test creates is owned by
Administrators (`BA`). The test's "wrong owner" case used the owner list
`["SY","BA"]`, so the owner was accepted and the store correctly refused the
inherited permissions instead ("unexpected permissions" rather than "unexpected
owner"). A test assumption, not a product defect; it only held when unelevated. Fix:
the case now uses an owner list that matches nobody. Passes locally (unelevated, 8/8
store tests); **not yet re-run on CI**.

Side benefit: everything else in that job passed as an administrator, not just as
my unprivileged user: the 8 pipe tests (including `users_cannot_add_a_pipe_instance`
and the production DACL read-back), the 4 service tests and 7 of 8 store tests.
Still not SYSTEM, and still not under the SCM.

## 2026-11-17 — `aegis-tunnel` host exe: argument rules and loading tunnel.dll, unelevated

New `aegis-tunnel.exe` (what each `WireGuardTunnel$aegis-…` service will run:
`/service <config>`). `fmt`, `clippy --workspace --all-targets -D warnings`,
`cargo test --workspace` pass; `aegis-service` tests: 8 pipe, 4 service, 8 store,
3 tunnel-host (+1 ignored, run explicitly below).
- Argument rules: only `/service <absolute path>` to a file named exactly
  `aegis-<24 hex>.conf.dpapi` directly inside the Aegis config directory is accepted
  (directory compared case-insensitively, file name exactly). Refused: no or extra
  arguments, other flags, relative paths, `..`, another directory or subdirectory,
  plaintext `.conf`, `wg0.conf.dpapi`, trailing `.exe`, empty stem, upper-case names.
- Loader failure paths: relative path, missing file, and `kernel32.dll` (no
  `WireGuardTunnelService` export) are all refused with errors; nothing is called.
- **Real `tunnel.dll`** (`AEGIS_TUNNEL_DLL=… cargo test … -- --include-ignored`):
  loads by absolute path with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
  LOAD_LIBRARY_SEARCH_SYSTEM32` and the export resolves. It was NOT called.
- **Defect found:** with `FreeLibrary` in `Drop`, the test process exited with
  `STATUS_ACCESS_VIOLATION` after the tests passed (1 crash). A Go c-shared DLL
  cannot be unloaded safely. I removed the unload; 3 consecutive runs then exit 0.
  (Removal is the only change; I did not isolate the crash further.)

**Not run:** calling `WireGuardTunnelService` (needs the SCM), the SCM install of
the tunnel service, `SERVICE_SID_TYPE_UNRESTRICTED`, DLL search behaviour with a
planted DLL, running as SYSTEM.

## 2026-11-17 — tunnel.dll and wireguard.dll built/fetched (not loaded or run)

Go 1.27.4 (official zip, SHA-256 matched go.dev's published value) extracted to
`C:\Users\digit\tools\go-install`, not on PATH, no installer, no admin. From
wireguard-windows `6ece77b`, built only the amd64 `tunnel.dll` with the same
command and flags as `embeddable-dll-service/build.bat`
(`-buildmode c-shared -trimpath -ldflags="-w -s"`, cgo with llvm-mingw 20260311,
`GOTOOLCHAIN=local`; module downloads checked by the repo's `go.sum`). The repo's
own `..\build.bat` was not run (it also builds the GUI for 3 architectures).
Inputs pinned by the upstream script and verified by SHA-256: llvm-mingw
`dd4c67d9…b182e1`, wireguard-nt 1.1 `dceb30a9…f038dace`.

| file | SHA-256 | note |
|---|---|---|
| `tunnel.dll` (amd64, local build, unsigned) | `70e8d70361097d54a0a1245a1b99d363ab0e853a166919edca5bf2ef6def2082` | exports `WireGuardTunnelService`, `WireGuardGenerateKeypair` (from `tunnel.h`) |
| `wireguard.dll` (amd64, wireguard-nt 1.1) | `b1b85e072c45d81358be29d94c599dc76652f912be8c0f0a41e2d5d89a6461d3` | Authenticode valid, signer WireGuard LLC |

A successful build is not a test.

**Format compatibility, run (as the current user, no elevation):** a throwaway Rust
program (outside the repo) wrote a profile with the real `ConfigStore`; a throwaway
Go program calling upstream's `conf.LoadFromPath`, the exact function
`WireGuardTunnelService` calls, loaded it: name `aegis-d512…`, 1 peer, allowed IPs
`0.0.0.0/0`, endpoint `192.0.2.1:51820`, 1 address, 1 DNS, private key present
(not printed). So our DPAPI parameters and the `# aegis-profile-id` comment line
are accepted by WireGuard's real code. Negative controls with the same loader: the
same blob renamed to another tunnel name fails with "input name does not match the
stored name"; a truncated blob fails with "The data is invalid". Test keys are the
public WireGuard documentation examples.

**Not done:** loading `tunnel.dll` itself, DPAPI **as SYSTEM** (the real scope:
this proved user scope for my account only), a tunnel service under the SCM, the
wireguard-nt driver install. Those need an elevated shell or the VM.

## 2026-11-17 — Windows service: encrypted config store and Install/Load/Uninstall, unelevated

Windows 11 Pro, normal user (not SYSTEM). `aegis-service`: 20 tests pass (8 pipe,
8 store, 4 service). `fmt`, `clippy --workspace --all-targets -D warnings` and
`cargo test --workspace` pass.

**Source reading, not run:** wireguard-windows `6ece77b` (2026-09-22) shows
`tunnel.dll` loads `<name>.conf.dpapi` (DPAPI user scope, description = tunnel
name, no entropy) as well as plaintext `.conf`; its WFP firewall for full tunnels
is a dynamic session (fail-open if the tunnel service dies) with no LAN option.
See DECISIONS D16. Nothing was built or executed from that source; `tunnel.dll`
itself has never been loaded here (no Go toolchain).

Store, with real DPAPI and real directory ACLs (as the current user):
- Stored file is exactly `aegis-<24 hex>.conf.dpapi`; its bytes contain none of
  the private key, `PrivateKey`, `[Interface]` or the endpoint; the config
  round-trips and still parses. Saving again replaces it and leaves no `.tmp`.
- A blob copied under another profile's name is refused (description mismatch),
  as `tunnel.dll` would; damaged and foreign files are skipped by `list`, and
  saving over a damaged file repairs it.
- Two profile ids sharing the first 24 hex digits cannot overwrite each other.
- The directory is created with `D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)` (+ owner ACE
  in tests) and is accepted again on restart. A directory someone else
  pre-created (inherited DACL) is refused, both for a foreign owner and for the
  wrong permissions.

Handler through the real pipe and client: Install/Load/Installed/Uninstall round
trip; Uninstall is idempotent. The service re-validates, bypassing the app:
`PostUp`, `Table`, a missing `Address`, non-config text and a ~100 KB config are
all refused, nothing is stored, and the error never contains the key or the
rejected value. Capabilities still report tunnels `Unsupported`, and `Activate`
fails even for an installed profile.

**Not run:** the production directory policy (needs SYSTEM or Administrators as
owner), `ProgramData` as the real location, DPAPI decryption by `tunnel.dll`,
a profile id re-import after a service restart under the SCM, the service as
SYSTEM.

## 2026-11-17 — Windows service pipe server (`services/windows-service`), unelevated

Windows 11 Pro, normal (non-elevated) user, so the server did **not** run as
SYSTEM. 8 integration tests in `services/windows-service/tests/pipe.rs` pass, on
unique test pipe names (never the production name), through the real
`aegis_windows::call_pipe`/`open_client` client code:
- `Hello` ok; wrong protocol version refused with a "reinstall" message.
- `Capabilities` reports `WindowsService`, not simulated, and **every** capability
  `Unsupported` (the skeleton claims nothing); `Install`, `Activate` and
  `ApplyKillSwitch` return errors instead of pretending.
- A second server cannot bind an owned pipe name (first-instance flag).
- An oversized length prefix, a non-JSON frame and a connect-and-vanish client do
  not stop the server; 20 requests on one connection and 16 parallel connections
  work.
- `production_dacl_is_applied_as_written`: the DACL read back from the pipe handle
  is `D:P(D;;FA;;;NU)(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x12019b;;;IU)`, i.e. no silent
  default DACL.
- `users_cannot_add_a_pipe_instance`: with only the interactive-user ACE, creating
  a second instance fails with `ACCESS_DENIED`.

**Wrong assumption found and corrected:** I expected a client opening the pipe
with `GENERIC_WRITE` to be denied because the DACL lacks
`FILE_CREATE_PIPE_INSTANCE`. A throwaway probe showed every access mask opens
(even `FILE_APPEND_DATA` alone). Windows enforces that right only when adding a
server instance, which is the property we need (no fake instances). The probe was
deleted; DECISIONS D16 and code comments state the measured behaviour.

Also: `cargo fmt`, `cargo clippy --workspace --all-targets -D warnings` and
`cargo test --workspace` pass; `cargo check -p aegis-service -p aegis-windows
--target x86_64-unknown-linux-gnu` passes (the crate is an empty stub there; only
type-checked, not tested or linted on Linux). `cargo audit` (1509 advisories): no
vulnerabilities in 516 crates; 4 "unmaintained" warnings, all in the Linux
zbus/zvariant/proc-macro-error stack that was already in the tree. `windows`
0.62.2 (Microsoft) and `windows-service` 0.8.1 (Mullvad) have no advisories.

**Not run:** the service under the Service Control Manager or as SYSTEM
(`aegis-service service`; the SCM entry point is compiled only); the pipe as
SYSTEM; a pipe client from a second user or a remote machine; the per-connection
client check (not written); DPAPI, tunnel install, `tunnel.dll`, kill switch.
`aegis-service console` was started by hand and the pipe appeared, nothing more.

## 2026-11-17 — Windows 11 host, handoff Step 0 (toolchain and baseline)

Host: Windows 11 Pro, Rust 1.99.1 (`stable-x86_64-pc-windows-msvc`), VS 2022
Build Tools, WebView2 present, Node 26.13.1, pnpm 12.21.0 (installed globally;
it was missing). Native, not cross-compiled.

Passed natively: `cargo fmt --all --check`; `cargo clippy --workspace
--all-targets -- -D warnings`; `cargo test --workspace` (config 22, core 61,
desktop 2, helper 3, windows 9 = 97 passed; `aegis-linux` has 0 tests on
Windows because its code is Linux-only); `cargo check -p aegis-desktop` (the
Tauri build script failure seen when cross-checking from Linux does not occur
natively). Frontend: `pnpm lint`, `pnpm typecheck`, `pnpm test` (28 passed).

Found and fixed on this host:
- **clippy `large_enum_variant` in `AnyBackend`** (`src-tauri/src/backend.rs`).
  On Windows the Linux `Nm` variant is compiled out, leaving `MockBackend`
  (312 bytes) beside 24-byte variants; Linux builds never saw it. `Mock` is now
  boxed.
- **CRLF checkouts broke `pnpm lint`** (Biome formatter wants LF; Git's
  `core.autocrlf=true` default on Windows rewrote every file). Added
  `.gitattributes` (`* text=auto eol=lf`), which also keeps the shell scripts
  usable from a Windows checkout.

Not explained: one `cargo test --workspace` run failed while compiling the
third-party `indexmap` crate (rustc exit 1, no diagnostic captured). It did not
recur on a rebuild and disk space was fine; treat as a one-off, not as fixed.

**Not run:** `pnpm tauri dev` (with or without `AEGIS_BACKEND=mock`), so the
UI has not been looked at on Windows; nothing about the pipe client against a
live service; no service, tunnel, DPAPI, SCM, tunnel.dll or WFP code exists or
ran. Biome prints a deprecation notice for `linter.recommended` in
`biome.json` (not an error; not changed).


## 2026-11-17 — Windows backend, step 1 (protocol, naming, pipe client)

Host (Linux): `cargo test -p aegis-windows` 9 passed: frame round trip, redacted
`Debug` of configs, oversized frame refused before allocating, garbage and
truncated frames are errors, clean EOF, error kinds survive the wire, tunnel
names within WireGuard's 32-char limit and ownership shape. `cargo clippy
--workspace --all-targets -D warnings` clean; `cargo test --workspace` 120 passed;
`cargo clippy -p aegis-windows --target x86_64-pc-windows-msvc` clean.
**Not run:** anything on Windows. The pipe client, service, DPAPI, SCM, tunnel.dll
and WFP kill switch are unexecuted or unwritten. A compile is not a test.

## 2026-11-17 — Sleep/resume, network switching, silent server; helper handshake; wrapper fix

**Defect found and fixed first:** the desktop's `AnyBackend` wrapper did not
forward `apply_kill_switch`, `remove_kill_switch` or `kill_switch_active`; the
trait defaults refused, so the kill switch of 2026-11-16 would have failed to
turn on in the real app (the VM suite drives `NmBackend` directly and did not
see it). Those three trait methods now have no default, so a missing forward is
a compile error, and `backend::tests` checks the wrapper passes them through.

Host: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
pass; `cargo test --workspace` 111 passed (core 62 incl. 7 new: network change
discards old proof and recovers, own-activation noise ignored, resume restarts
a session that ran out of retries, resume never revives a user-ended or
never-connected session, kill switch stays on through sleep/resume, a silent
server is reported and recovers while an idle tunnel is not; helper 3;
desktop wrapper 2). Frontend biome, tsc, `vitest run` 28 passed. Windows
type-check: `cargo check -p aegis-windows --target x86_64-pc-windows-msvc` pass.
`shellcheck`/`shfmt -d` pass on the VM and install scripts.

VM (`scripts/vm/run-vm-tests.sh`, Debian 13, NM 1.52.1): **215 PASS, 0 FAIL**,
`ALL PASSED` in `full` and `full4`. New in this run, per mode:
- Handshake time read as root through the helper (`wg show … latest-handshakes`),
  reported `Supported`, and 8–12 s old when sampled.
- Network switch with the kill switch on: a better-metric dummy uplink in
  NetworkManager changes the primary connection; the engine logged "The network
  changed", re-confirmed with fresh data, kept routing through the tunnel and
  the block, and nothing was seen on the uplink (tcpdump). Switching back did
  the same.
- Silent server: stopping the server's WireGuard interface inside the test
  namespace while the client kept pinging turned the peer to `NoResponse`
  within 60 s (tunnel kept, "stopped answering" logged, kill switch still
  blocking, nothing on the uplink); restarting it returned to `Confirmed`.
- Routes, rules, DNS and nftables identical to the baseline afterwards.
- A hand experiment (`events_probe` in the VM) showed the watcher receiving
  `NetworkChanged` bursts (3 at once on a switch, 1 on switch-back).

Earlier runs of this suite version had two test-side failures, fixed before
the final run: the "stopped answering" assertion ran after a second network
switch had reset the peer to Pending (test now re-confirms first), and a
settings change returned `Busy` while the monitor held the operation lock
(test now retries).

Not tested: a real suspend/resume (the resume path is unit-tested with an
injected event and the logind watcher is untested end to end), a real Wi-Fi or
Ethernet switch, and whether NetworkManager re-adds the endpoint route after a
real uplink change. The new `handshake` helper command needs the helper
reinstalled on machines that installed the earlier one.

## 2026-11-17 — Locations feature, final combined-workspace validation

- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- `cargo test --workspace --all-targets --locked`: **102 passed**, 12 suites.
  Includes portable config/core, Linux mapping/checks, helper unit checks and
  GLib source-pin tests. Mock engine tests are not real tunnel validation.
- `cargo build -p aegis-desktop --locked`: pass (Linux desktop executable).
- Frontend `biome check src`, `tsc --noEmit`, `vitest run`, `vite build`:
  pass; **27 tests** across 2 files. Biome retains its existing config
  deprecation notice. Used installed `node_modules/.bin` tools because the
  global pnpm launcher returned `Exec format error`; no dependency changes
  were needed for Locations.
- `cargo check -p aegis-core -p aegis-windows --target
  x86_64-pc-windows-msvc --locked`: pass. This checks Windows-target source,
  not a Windows executable/runtime VPN. The Windows tunnel service is still
  unimplemented; Windows CI builds/portable tests remain configured.
- Source assertions: all **249** ISO catalog codes have bundled flag assets;
  every declared Tauri command has a handler and explicit permission; no broad
  shell/fs/http/dialog JS permissions were added. `git diff --check`: pass.
- Optional Jev completion-check preview was rejected by automatic approval
  review over potential external disclosure of repository/test details. It was
  not retried or bypassed; completion boundaries rely on local checks above.
- Independent code review found lost preflight cancellation and a disabled
  Cancel button; fixed and regression-tested. A follow-up found failed protected
  retry misclassified as switching; fixed. Protected active country switches
  remain refused, with tests asserting no tunnel/block mutation. Preference
  changes now emit the event needed to refresh cached profile views.
- Earlier UI checks failed on ambiguous test queries and the corrected DNS
  resolver-egress label; assertions were corrected and final suites passed.
  A combined run also caught the concurrently edited cancelled-reconnect
  cleanup regression; its targeted test and subsequent full suite passed.
- Read-only `nm_probe` ran on 2026-11-16: NM 1.58.1 and an existing Up tunnel
  observed. No host profile, route, firewall or tunnel was changed by this
  feature's validation.
- `scripts/vm/run-vm-tests.sh`: **could not start**, `/dev/kvm` unavailable in
  this execution environment. Extended country-profile switch/traffic checks
  are in `vm_suite`, and the manual `vpn-integration.yml` workflow is configured.
  Separately recorded VM runs below are not claimed as this session's live
  validation. No live Windows, independent public multi-country server,
  protected-switch, suspend/reboot/uplink-change or visual desktop check ran.

The Locations UI/registry and unprotected switching integration are implemented;
full cross-platform/protected-network acceptance is **not complete**. Existing
private/preshared keys enter through native `.conf` import; manual setup uses
Rust-generated keys and explicit remote-registration confirmation. RTT remains
unmeasured; provisioning and geolocation are not implemented.

## 2026-11-17 — Audit fixes with the kill-switch integration

Two integration regressions were reproduced with mock backends before their
fixes, then passed:

- Cancelling a scheduled reconnect after backend deactivation fails must
  retain the firewall block and `Uncertain` state; a later successful Disconnect
  removes the block. All three reconnect cancellation paths now require
  successful teardown before removing it.
- Recovery with a supported helper but no installed block must keep the live
  tunnel `Uncertain`, refuse unprotected reactivation after a drop and allow
  a fresh protected connection after Disconnect. Session checks now require
  an installed block as well as backend support.

Combined working-tree checks (includes concurrent work):

- `cargo test --workspace --all-targets --locked`: **100 passed** (12 suites).
- `cargo test -p aegis-desktop --test glib_security --release --locked`:
  **2 passed**, retaining the optimized GLib iterator and source-pin checks.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- `cargo fmt --all --check`: pass after formatting the new Rust assertion.
- Frontend Vitest: **27 passed**; TypeScript and Biome passed. A transient
  formatting failure in the concurrently edited `App.test.tsx` was gone on
  the final check; Biome retains its existing configuration deprecation notice.
- Focused Jev Review ran twice. Its scores are advisory; the mock regressions
  and project checks provide the executable evidence.

Attempted `bash scripts/vm/run-vm-tests.sh`: exited 1 before booting because
`/dev/kvm` is absent in this environment (runner: "no write access to
/dev/kvm"). No host network or firewall changes were made. The earlier VM
results below are historical evidence, not a live rerun of these changes.
No IPv6 implementation was changed in this follow-up.

Not rerun here: Windows, real Polkit prompt/desktop flow, dependency audit,
yanked-release checks or Gitleaks. The new root helper was not independently
security-audited by this focused core follow-up.

## 2026-11-16 — Kill switch (aegis-helper, pkexec, nftables `inet aegis_vpn`)

Host:
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
  `cargo test --workspace`: 95 passed (incl. helper argv/ruleset tests and 5
  engine kill-switch tests: block before activation, kept after drop and
  failed attempt, removed on Disconnect, hostname endpoints refused, failed
  apply never activates, leftover block adopted at startup).
- `nft` accepted the generated ruleset and the helper's read-back passed in an
  isolated user+network namespace (`unshare -rn`, host firewall untouched);
  marker-write failure rolls the table back (ruleset empty afterwards).
- Frontend biome, tsc, `vitest run`: 25 passed (new: "Stop blocking" shown and
  wired while the kill switch blocks after a failure).
- `shellcheck`, `shfmt -d` on `scripts/install-helper.sh`, `scripts/vm/run-vm-tests.sh`: pass.

VM (`scripts/vm/run-vm-tests.sh`, Debian 13, NM 1.52.1, helper + Polkit policy
installed at package paths, test-only Polkit rule for the SSH user):
**167 PASS, 0 FAIL**, `ALL PASSED` in `full` and `full4`. Per mode:
- Kill switch reported `Supported`, then `Verified`; table present; server
  confirmed and sentinel reachable through the block.
- Uplink tcpdump (6 s window, exit 124 = nothing seen): no packets from
  `ping -I <uplink> 1.1.1.1`; LAN (slirp gateway 10.0.2.2) blocked.
- After `nmcli connection down` (auto-reconnect off): state Failed, table still
  present, still nothing on the uplink.
- Disconnect removes the table; positive control: the same forced ping is
  then seen on the uplink.
- Allow local network: LAN packets seen, forced internet ping still blocked.
- Routes, rules, DNS and nftables identical to the baseline afterwards.

Not tested: real desktop Polkit prompt flow (VM used a test rule), app crash
with block left behind (unit-tested with the mock only), reboot, tray UI.

## 2026-11-16 — IPv6 block as tunnel device route; reachability (combined working tree)

Fixes the leftover found in the run recorded further down ("DNS/IPv6
protection (block + runtime verify)"): IPv4-only full tunnels now route
`::/0` into the tunnel device (metric 1) instead of a `type=unreachable` route
on `lo`. Chosen by a manual VM experiment (connect, restart NM, disconnect):
both `unreachable` variants (method manual / link-local) left the route
behind; the device route left nothing.

- `cargo clippy --workspace --all-targets -- -D warnings`: pass (after fixing
  an unused `mut` in `vm_suite`). `cargo test --workspace`: pass (config 22,
  core 43, linux 20 incl. `/proc/net/ipv6_route` parsing and IPv6 judging,
  glib_security 2).
- VM (`scripts/vm/run-vm-tests.sh`, Debian 13, NM 1.52.1, kernel
  6.12.116): **122 PASS, 0 FAIL**, `ALL PASSED` in `full` and `full4`.
  - full4: `ip -6 route get` → `dev aegis-… metric 1`; ping6 fails; tcpdump
    on uplink `ens3` captured no IPv6 packets (ran full 6 s, exit 124); IPv6
    reported `Verified` ("sent into the tunnel … dropped"); pre-blocking
    install refreshed on connect; **nothing left after NM restart cycle**.
  - full: IPv6 via tunnel, `Verified`. DNS `Verified` in both modes.
  - Reachability: `Verified` with tunnel DNS answering; `Failed` ("no
    answer … within 3 seconds") after stopping the tunnel's dnsmasq. Both modes.
- Not run: frontend checks after this change (no frontend files changed since
  the 23-pass run), tray/notifications, Windows.

## 2026-11-16 — Final audit-only security validation

Scope: the user chose to keep the audit fixes and leave IPv6 blocking to the
other session. Validation used a frozen `/tmp/aegis-audit-validation` copy of
the audit fixes against `b93f017`, excluding concurrent IPv6 blocking, runtime
verification, server registry and UI redesign changes. Results below apply
to that snapshot, not the combined working tree. Earlier entries remain as
the record of their respective runs.

- `cargo test --workspace --all-targets --locked`: **67 passed**.
- `cargo test -p aegis-desktop --test glib_security --release --locked`:
  **2 passed**, including the Git source pin and optimized iterator regression.
- Frontend Vitest: **10 passed**. TypeScript, Biome, workspace/all-target Clippy
  with warnings denied and Rust formatting passed. Biome retains its existing
  configuration deprecation notice.
- Shellcheck, shfmt and the unprivileged peer-cleanup stub test passed.
- Disposable Debian 13 VM, kernel 6.12.116+deb13-cloud-amd64, NetworkManager
  1.52.1, systemd 257: **91 PASS, 0 FAIL**, both `full` and `full4` reported
  `ALL PASSED`, runner exited 0. DNS positive controls, zero uplink DNS packets,
  reconnect, three NetworkManager restarts per mode and exact route/rule/DNS/
  nftables cleanup passed. The guest and temporary runner files were removed.
  The audit-only IPv4 mode still reports IPv6 protection as failed; it does
  not install the other session's blocking route.
- Cargo audit using the refreshed local database, `--no-fetch --no-yanked`:
  zero classified vulnerabilities, four unmaintained-package warnings. The
  narrow GLib exception remains conditional on the pinned fixed source.
- `/tmp/aegis-audit-validation/audit-fixes.patch`: applies to `b93f017` and
  reproduces all 27 tested patch files byte-for-byte; recorded source hashes
  match. This is a local review artifact, not a commit.

An intermediate VM run failed after strict D-Bus error propagation exposed
stale ActiveConnection paths following a NetworkManager restart (eight failed
checks across the two modes). The final implementation checks a fresh
ObjectManager snapshot for object absence; the rerun above passed all cycles.
Concurrent incomplete registry changes also prevented a mixed-workspace
build/typecheck, which is why final evidence was isolated rather than attributed
to that integration.

Not checked: combined-session integration, Windows runtime/ACL behavior, live
webview penetration, yanked crates or Gitleaks. Jev Review remained unavailable
(`JEV_API_KEY` unset). GLib uses an unmerged compatible backport; library-owned
D-Bus serialization buffers cannot be guaranteed wiped. See
[GLIB_BACKPORT.md](../security/GLIB_BACKPORT.md).

## 2026-11-16 — Dashboard last response + public IP

- `cargo test -p aegis-core -p aegis-linux --lib --tests`: 62 passed.
- `cargo clippy -p aegis-core -p aegis-linux -p aegis-desktop --all-targets -- -D warnings`: pass.
- Frontend Vitest: 24 passed (connected handshake + public IP display).
- Not re-run: VM harness, full workspace.

## 2026-11-16 — DNS/IPv6 protection (block + runtime verify) and internet reachability

Host: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
pass; `cargo test --workspace` pass (aegis-core 42, aegis-linux 18 incl. new
verify/settings tests); frontend biome, tsc, `vitest run` 23 passed.

VM (`scripts/vm/run-vm-tests.sh`, Debian 13, NM 1.52.1, systemd 257), 120 PASS / 1 FAIL:
- full and full4: DNS protection `Verified` at runtime (systemd-resolved `~.`
  only on tunnel, server routed via tunnel); 0 DNS packets on uplink.
- full: IPv6 `Verified` (routed through tunnel). full4: IPv6 now blocked
  (`ip -6 route get` → No route to host, ping6 fails), reported `Verified`;
  a pre-blocking NM install (ipv6 disabled) is refreshed on connect.
- Reachability: `Verified` with tunnel DNS answering; `Failed` ("no answer …
  within 3 seconds") after stopping the tunnel's dnsmasq. Both modes.
- **FAIL (full4):** after the "NetworkManager restart while connected" cycle,
  disconnect leaves `unreachable default dev lo proto static metric 1`
  behind, so IPv6 stays blocked after disconnecting (fail-closed, not a
  leak). NM loses ownership of the route across its restart. Not fixed yet.
- Earlier baseline run (before these changes) also had a separate failure:
  disconnect after NM restart hit a vanished ActiveConnection object
  ("No such interface … ActiveConnection/2"); it did not recur in this run.

## 2026-11-16 — Security review follow-up (honesty gaps)

- `cargo test -p aegis-core -p aegis-linux --lib --tests`: 43 passed (includes
  new cases: recovered kill-switch session must not reactivate after drop;
  activation failure with failed cleanup stays Uncertain and retryable;
  absent D-Bus name detection).
- `cargo clippy -p aegis-core -p aegis-linux --all-targets -- -D warnings`:
  pass.
- Not re-run this turn: full workspace suite, VM harness, frontend Vitest,
  GLib optimized test, shellcheck.

## 2026-11-16 — Security fixes

- `cargo test --workspace --all-targets --locked`: 61 tests passed, including
  configuration validation, protection reporting, failed disconnect/switch,
  session protection policy, recovery, status reconciliation, nonregular
  imports, VM snapshot/capture checks and GLib source-pin/iterator checks.
- `cargo test -p aegis-desktop --test glib_security --release --locked`:
  2 passed with the pinned compatible backport in an optimized build.
- Frontend Vitest: 11 passed, including unknown-status Disconnect availability.
- Workspace/all-target Clippy, TypeScript and frontend lint: pass. Biome emits
  the existing configuration deprecation notice.
- Shellcheck and shfmt on the peer script, its cleanup test and VM runner: pass.
  `bash scripts/test-netns-peer.test.sh`: pass; writable PID file ignored in
  favor of stubbed kernel namespace PIDs. No host networking changes.
- `scripts/vm/run-vm-tests.sh`: **91 PASS, 0 FAIL**, `ALL PASSED` for `full`
  and `full4`, exit 0. Guest: Debian 13, kernel
  6.12.116+deb13-cloud-amd64, NetworkManager 1.52.1, systemd 257. Both modes:
  tunnel DNS answers, 0 uplink DNS packets with 4-packet positive controls,
  reconnect and three NM restart cycles passed; routes/rules/DNS/nftables
  restored exactly. No broad XFAIL acceptance of firewall leftovers remains.
  The runner powered off the guest and removed its temporary files.
- Cargo audit, local refreshed database and `--no-fetch --no-yanked`:
  0 classified vulnerabilities; 4 unmaintained warnings. Narrow GLib exception
  is guarded by its source pin and optimized test, not scanner silence alone.
  See [GLIB_BACKPORT.md](../security/GLIB_BACKPORT.md). Yanked crates unchecked.

New config/engine regressions failed before fixes. During validation the GLib
source-pin test caught mixed Git/registry sys packages; the lockfile was unified
before retaining the patch, and the desktop/optimized checks then passed.
No Windows runtime/ACL test, webview penetration test or Gitleaks scan.
Jev Review was unavailable (`JEV_API_KEY` not configured). D-Bus library buffers
cannot be guaranteed wiped by application-owned key types.

## 2026-11-16 — UI redesign (tunnel path dashboard, ink/status palette, bundled Atkinson fonts)

| Check | Command | Result |
|---|---|---|
| Lint | `biome check src` | pass |
| Types | `tsc --noEmit` | pass |
| Frontend tests | `vitest run` | 10 passed (new: tunnel path never lights internet reachability) |
| Build | `vite build` | pass, 267 KB JS (83 KB gzip), fonts bundled locally (CSP `font-src 'self'`) |

Not run: visual review of the running app (no screenshots by policy); `tauri dev`.

## 2026-11-16 — Security source review at b93f017

- `cargo test --workspace`: 51 passed.
- `node node_modules/vitest/vitest.mjs run` in `apps/desktop`: 9 passed.
- Isolated `/tmp/aegis-security-probe` Cargo project: 8 probes passed, confirming
  defective behavior in configuration mapping, requested protection, stale
  status and deactivation handling. These are reproductions, not security fixes.
- Fresh `cargo audit -d /tmp/aegis-security-advisory-db --json --no-yanked`:
  0 classified vulnerabilities; 1 unsoundness warning (GLib 0.18.5) and
  4 unmaintained-package warnings. Yanked releases were not checked.
- `sh /home/mikeh/.npm-global/bin/pnpm audit --json` in `apps/desktop`:
  0 advisories, including development dependencies.

Report and suggested fixes: [security_best_practices_report.md](../../security_best_practices_report.md).
No disruptive VM/host-network test, privileged exploit test, Windows runtime
check or Gitleaks scan was run during this audit. No application fixes applied.

## 2026-11-16 — VM suite (`scripts/vm/run-vm-tests.sh`): full tunnel, leaks, recovery

Host: QEMU 11.1.2 + KVM, user-mode networking, unprivileged. Guest: Debian 13
(trixie) genericcloud, SHA512-verified image, kernel 6.12.116, systemd 257,
NetworkManager 1.52.1, systemd-resolved. Peer: `test-netns-peer.sh` modes
`full` (0.0.0.0/0 + ::/0) and `full4` (0.0.0.0/0 only) inside the VM.

**Final run (run 7): 89 PASS, 0 FAIL, harness exit 0.** Per mode:

| Check | full | full4 |
|---|---|---|
| nftables snapshot works; DNS capture positive control (4 packets on uplink before connect) | PASS | PASS |
| Connect; server response confirmed | PASS | PASS |
| IPv4 internet route via tunnel; endpoint route outside tunnel | PASS | PASS |
| Ping IPv4 sentinel that exists only behind the tunnel | PASS | PASS |
| IPv6 route via tunnel + IPv6 sentinel ping | PASS | n/a |
| IPv6 bypasses IPv4-only tunnel; app reports IPv6 protection **Failed** | n/a | PASS |
| DNS protection reported Configured | PASS | PASS |
| resolvectl and libc lookups answered by tunnel DNS; **0 DNS packets on uplink** | PASS | PASS |
| External `nmcli connection down` → engine reconnects and re-confirms | PASS | PASS |
| Disconnect: IPv4/IPv6 rules, routes, resolved DNS/domains and nftables identical to baseline | PASS | PASS |
| 3× (connect, restart NetworkManager, re-confirm, routed via tunnel, ping) | PASS | PASS |
| After restart cycles: nothing left behind | XPASS (leak not reproduced) | XPASS |
| Delete; no leftover NM connection; peer down | PASS | PASS |

### Finding: NetworkManager leaks its WireGuard anti-spoofing rule (DECISIONS D11)
Run 4 failed IPv4-only checks because NetworkManager 1.52.1 left raw-table
rules `ip daddr 10.99.0.2 iifname != "aegis-<old>" … drop` from earlier tunnels,
silently dropping replies for later tunnels with the same address (while
encrypted data still arrived, so "server responded" was true). Each leaked
interface belonged to a cycle where NetworkManager was restarted while the
tunnel was up. Reproduced without Aegis using `nmcli connection import`:
normal up/down → 0 rules left; up, NM restart, down → 1 rule left.
Frequency: leaked 5 of 5 times in run 4, its 2 reruns and the manual test; 0 of
10 restart cycles in runs 5–7. Unverified hypothesis: it happens when the
restart comes within ~1–2 s of activation. Decision: detect and warn via the
future helper; never remove.

### Harness bugs found and fixed while getting here (no product impact)
- Runs 1–3: VM had no IPv4. Cause: `-netdev user,ipv6=on` without `ipv4=on`
  disables IPv4 (QEMU: naming one protocol disables the other). Also moved the
  cloud-init seed from an HTTP server to a CIDATA ISO.
- Run 4: snapshot comparison did not include nftables (could not see the leak)
  and included self-changing values (RA lifetimes, counters, nexthop ids).
- Run 5: baseline taken before the peer veth finished IPv6 DAD.
- Modes now use distinct client addresses (10.99.0.3 / .4) so one mode's
  leftovers cannot affect the next.

Not covered: kill switch (not implemented), suspend/resume, reboot with tunnel
active, uplink/Wi-Fi changes, desktop UI driving the connection, other distros.

## 2026-11-16 — VM harness pre-push checks

- `shellcheck scripts/test-netns-peer.sh scripts/vm/run-vm-tests.sh`: pass.
- `shfmt -d scripts/test-netns-peer.sh scripts/vm/run-vm-tests.sh`: pass.
- `cargo fmt --all --check`: pass.
- `cargo test -p aegis-linux`: 8 tests passed.
- `cargo clippy -p aegis-linux --all-targets -- -D warnings`: pass.

Not run in this check: the disruptive VM suite. These checks do not establish
full-tunnel, DNS/IPv6 leak, reconnect or NetworkManager-restart results.

## 2026-11-16 — LIVE edit + export through NetworkManager

Same host and isolated netns peer as the split-route run below (peer re-created
with fresh keys). Test: `target/debug/examples/nm_edit_live
/run/aegis-test/aegis-netns-test.conf`, unprivileged user.

| Step | Result |
|---|---|
| Editor draft loads (NM `GetSecrets`) | PASS |
| Draft shows correct derived public key | PASS |
| Draft JSON (UI payload) contains no private key | PASS (384 B) |
| Update: rename, MTU 1380, keepalive 15, add search domain (NM `Update2`) | PASS |
| Reloaded config has the edits and the original private/peer keys | PASS |
| Connect edited profile | PASS |
| Kernel interface `aegis-43bc7148` MTU is 1380 | PASS |
| Ping 10.99.0.1 through edited tunnel; server response confirmed | PASS |
| Export; file mode 0600; parses equal to edited config | PASS |
| Aegis NM keyfile exists and is not readable by the user | PASS |
| Delete; no leftover NM connection or interface | PASS |

First attempt had 1 FAIL: the test asserted the keyfile **directory** was not
listable, but Debian's `network-manager` package ships
`/etc/NetworkManager/system-connections` as 0755 (files 0600). That was a wrong
assertion, not a leak: no keyfile was readable. The test now checks the
real property (our keyfile unreadable). Consequence recorded in the threat
model: other local users can see profile *names* via the directory listing.

## 2026-11-16 — LIVE split-route tunnel through NetworkManager (first real tunnel)

Host: Debian forky/sid, KDE Plasma Wayland, kernel 7.2.15, NetworkManager 1.58.1,
Tailscale active. Peer: `scripts/test-netns-peer.sh up` run via `pkexec`
(isolated namespace `aegis-peer`, WireGuard server 10.99.0.1, endpoint
10.200.0.2:51820 over veth). Test: `target/debug/examples/nm_live
/run/aegis-test/aegis-netns-test.conf`, run as the normal (unprivileged) user.

| Step | Result |
|---|---|
| Import profile | PASS: route kind Split |
| Connect (NM AddConnection2 + ActivateConnection via Polkit, no root) | PASS in 282 ms |
| `ping -c 3 10.99.0.1` through the tunnel | PASS: 3/3 received |
| Engine reports server response confirmed | PASS: `Connected { peer: Confirmed }`, interface `aegis-874e8416`, rx 476 B / tx 564 B |
| Disconnect | PASS: state Disconnected |
| Delete profile | PASS |
| No leftover `aegis-*` interface | PASS |
| No leftover 10.99.0.0/24 route | PASS |
| No leftover Aegis NetworkManager connection | PASS: 0 |

Host state before vs after (`ip route/rule show table all` v4+v6, `resolvectl dns`,
`nmcli connection show`): policy rules **identical**, DNS **identical**, NM
connection list **identical**. Routes differ only by the test peer's own
`aegis-veth0` link routes (created by the script, removed by `down`).
After `test-netns-peer.sh down`: routes and policy rules (v4+v6, all tables)
**identical** to the pre-test snapshot.

Not covered by this run: full-tunnel routing, DNS/IPv6 leak behaviour, kill
switch, reconnect after a real drop, NM restart, suspend/resume, reboot, and
the desktop UI driving the connection (the test drives the same `Engine` the
UI uses). Verification of the NM keyfile permissions was not done.

## 2026-11-16 (later) — tauri-plugin-notification → notify-rust 4.19

| Check | Command | Result |
|---|---|---|
| Format / clippy | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| Tests | `cargo test --workspace` | 51 passed |
| zbus 5 dependents | `cargo tree -i zbus@5.19.0` | only tauri-plugin-single-instance 2.5.5 remains |
| Rust advisories | `cargo audit` | 0 vulnerabilities; same 5 warnings (zbus 5 crates now via single-instance only; proc-macro-error; glib 0.18) |
| Windows type-check | `cargo check -p aegis-desktop --target x86_64-pc-windows-msvc` | not possible here: tauri-build needs `llvm-rc` (not installed). The `cfg(not(unix))` notification branch is unverified until the Windows CI job runs |

Not run: an actual notification popup on KDE/GNOME/Windows.

## 2026-11-16 (later) — zbus 5.19 → 6.3 migration, same host

| Check | Command | Result |
|---|---|---|
| Format / clippy | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| Tests | `cargo test --workspace` | 51 passed (21 config, 22 core, 8 linux) |
| NM read-only probe on zbus 6 | `cargo run -p aegis-linux --example nm_probe` | pass: NM 1.58.1, 0 Aegis profiles, no active tunnel |
| Rust advisories | `cargo audit` | 0 vulnerabilities; same 5 warnings. zvariant/zvariant_derive/zbus_names 5.x now come only from tauri-plugin-notification 2.8.1 and tauri-plugin-single-instance 2.5.5 (`cargo tree -i zbus@5.19.0`); aegis-linux no longer depends on them |

Not run: live tunnel test (still needs root for the netns peer).

## 2026-11-16 — Debian forky/sid, KDE Plasma (Wayland), kernel 7.2, NM 1.58.1, rustc 1.99.1, Node 24

| Check | Command | Result |
|---|---|---|
| Rust format | `cargo fmt --all --check` | pass |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| aegis-config | `cargo test -p aegis-config` | 21 passed (incl. 2000-case proptests, RFC 7748 key vector) |
| aegis-core | `cargo test -p aegis-core` | 22 passed (connect/cancel/reconnect/fail-closed/recovery/no-secret-leak) |
| aegis-linux | `cargo test -p aegis-linux` | 8 passed (NM settings mapping, ownership, DNS byte order) |
| Windows stub | `cargo check -p aegis-windows --target x86_64-pc-windows-msvc` | pass (type-check only, not linked or run) |
| Frontend lint/types | `pnpm lint`, `pnpm typecheck` | pass |
| Frontend tests | `pnpm test` | 9 passed |
| Frontend build | `pnpm build` | pass, 265 KB JS (83 KB gzip) |
| Desktop build | `cargo build -p aegis-desktop` | pass |
| App launch (mock) | `AEGIS_BACKEND=mock pnpm tauri dev` | launched, no errors in log; UI not visually reviewed |
| NM read-only probe | `cargo run -p aegis-linux --example nm_probe` | pass: connected, NM 1.58.1, 0 Aegis profiles, no active tunnel |
| Shell scripts | `shellcheck`, `shfmt -d` on `scripts/test-netns-peer.sh` | pass |
| Workflow syntax | `yq` parse of `.github/workflows/*.yml` | parses; actionlint not installed |
| Rust advisories | `cargo audit` | 0 vulnerabilities; 5 warnings: zvariant/zvariant_derive/zbus_names 5.x unmaintained (RUSTSEC-2026-0370/0371/0372, via zbus 5.19), proc-macro-error unmaintained (RUSTSEC-2024-0370), glib 0.18.5 unsound (RUSTSEC-2024-0429, via Tauri GTK3 stack) |
| npm advisories | `pnpm audit --prod` | no known vulnerabilities |

### Not run
- **Live tunnel test** (`nm_live` against netns peer): needs root for the namespace. Not run.
- CI on GitHub: repository has no remote.
- Windows build/run: no Windows host; CI job written but not run.
- Tray on GNOME/KDE, notifications, autostart, packaging (deb/rpm/nsis).
- Any DNS/IPv6 leak, kill-switch, suspend, reboot or network-change test.
- cargo-deny (not installed locally).

## 2026-11-17 — macOS installer, branding (Apple silicon, macOS 26, no root)

- `scripts/macos/build-pkg.sh` built `dist/VPNFlow-0.1.0.pkg` (arm64, unsigned): payload lists the app,
  `vpnflow-daemon`, `wireguard-go` (built from source) and the launchd plist.
- `plutil -lint` passes for the plist; `bash -n` passes for the scripts.
- `pnpm lint`, `pnpm typecheck`, `pnpm test` (30) pass after the palette/logo change.
- **Not run:** installing the pkg, starting the daemon, any tunnel, route, DNS or pf behaviour, x86_64/universal build.

## 2026-11-17 (later) — macOS live tunnel on the developer's Mac (Apple silicon, macOS 26)

Installed pkg, root daemon running, Proton free WireGuard configs (US-FREE-155, Tokyo/JP-FREE-2 seen).
Manual checks:
- Connected: public IP 37.19.205.196 (AS212238 Datacamp, Tokyo). Disconnected: real ISP IP returned.
- `scutil --dns` while connected: only 10.2.0.1. `utun8` MTU 1420.
- Kill switch ON: `pfctl -a com.apple/vpnflow -sr` shows endpoint/DHCP/utun allow rules, then `block drop quick all`.
  (Kill switch is off by default, so the anchor does not exist until enabled in Settings.)
- Cloudflare WARP conflicts with VPNFlow (both full-tunnel); WARP must be off.

`scripts/macos/verify-vpn.sh`, report `verify-report-20261117-222333.txt`: 10 passed, 2 failed.
- PASS: tunnel up, single DNS server, name lookups work, IPv4-only fallback (no IPv6 leak seen),
  kill-switch rules loaded, pf enabled, `pkill wireguard-go`: 2 probes blocked, 0 leaked, app reconnected in 6s.
- The 2 FAILs were a script bug (it looked for interface `utun`; the real one is `utun8`), not a product failure.
  Fixed in the script (finds the utun with an IPv4 address); the DNS-route and default-route checks need a re-run.
- **Not verified:** DNS route / default route checks (pending re-run), ipleak.net DNS result,
  behaviour on disconnect with kill switch on, server switch while connected, x86_64/universal build, Windows.

`verify-vpn.sh` re-run after the interface fix, report `verify-report-20261117-222445.txt`: **12 passed, 0 failed, 0 warnings.**
- DNS server 10.2.0.1 and the default route both go via utun8. Tunnel kill: 2 probes blocked, 0 leaked, auto-reconnect in 5s.
- Still **not verified:** ipleak.net browser DNS result, IPv6 on a network that has native IPv6 (this run fell back to IPv4),
  disconnect behaviour with kill switch on, server switch while connected, x86_64/universal build, Windows.

## 2026-11-17 (later) — live DNS and IPv6 protection status (macOS)

Added `crates/aegis-macos/src/verify.rs` and `verify_protection` in the macOS backend. On every status poll while connected it reads `scutil --dns`, `route -n get` and `ifconfig` and reports:
- DNS: Verified only if every non-scoped system DNS server routes through the tunnel interface; Failed (naming the server) otherwise.
- IPv6: Verified if the IPv6 route goes into the tunnel, there is no IPv6 route, or the network has no global IPv6 address; Failed if IPv6 routes outside the tunnel on a network that has it. Off for split or IPv6-only profiles.
- Disconnected: the engine already reports Off.

Run: `cargo test -p aegis-macos` 10 passed (5 new parser/judge tests), `cargo clippy -p aegis-macos --all-targets` clean, `cargo test --workspace` all passed.
Spot check of the inputs on the developer's Mac while connected: DNS 10.2.0.1, IPv6 route via utun8.
Not yet verified: the badges in the running app (needs a rebuilt and reinstalled app), and a Failed case on a native-IPv6 network.

## 2026-11-17 (later) — live reachability and network-drop statuses (macOS)

Cause of "Internet reachability: Checking" never resolving: the macOS backend did not implement `check_reachability`, so the engine kept the default (Off) while connected.

Changes: macOS now sends one DNS lookup (example.com) to the profile's tunnel DNS server and reports Verified/Failed; the route to that server must go through the tunnel. Engine re-checks every 10 s (was 60 s) and keeps re-checking after the peer stops answering. With no system DNS server (Wi-Fi off) the DNS and IPv6 badges show a neutral "waiting for the network" (Configured) instead of Failed.

Tests: aegis-macos 11 passed, aegis-core 69 passed, clippy clean. 3 aegis-helper (Linux) tests fail on macOS; unrelated, not touched.
Not yet verified live: Wi-Fi off/on with the new build.

## 2026-11-17 (later) — Wi-Fi drop test and speed panel

User confirmed on the rebuilt macOS app (their report, not a scripted run): all statuses correct when healthy; Wi-Fi off showed the right non-working statuses; Wi-Fi on returned everything to the proper status.

Added a Connection speed panel (Dashboard) and `speed_test` command: TCP connect latency/jitter to 1.1.1.1:443, 25 MB download (10 s cap) and 6 MB upload against speed.cloudflare.com. Runs only on button press.
Tests: speed math unit test, aegis-desktop 4 passed incl. IPC permissions, vitest 30 passed, tsc clean.
Not yet verified live: the speed test itself against a real tunnel (numbers, timeouts, error notes).

## 2026-11-17 (later) — speed panel live check (macOS)

Installed build matches the built binary (cmp). User ran Test speed once connected to JP-FREE#2 and sent a screenshot: download 80.4 Mbps, upload 16.1 Mbps, latency 183 ms, jitter 3 ms, no notes. Single run, one server; latency includes the path through the VPN server to Cloudflare, so it is not the Mac-to-server ping. Timeout and failure paths not exercised live.

## 2026-11-17: macOS sleep/wake and network-change events

- Added `crates/aegis-macos/src/events.rs`: polls every 2s; sleep = wall clock jumps ahead of monotonic clock (>=5s); network change = different non-tunnel addresses/default routes. Feeds the engine's existing `Resumed`/`NetworkChanged` reconnect handling (previously macOS emitted none).
- Ran: `cargo test -p aegis-macos` (13 pass, incl. 2 new), `cargo clippy -p aegis-macos` clean, release app built.
- NOT yet run live: sleep/wake reconnect, switching Wi-Fi networks while connected.

## 2026-11-17: first-run onboarding (built, not yet run live)

- Added `Onboarding.tsx` (5 steps: Welcome, Proton account, server files, countries, finish), `onboarding.rs` (fixed link allowlist; Downloads `.conf` watcher, each file offered once, only files newer than the step start), commands `open_link` and `scan_downloads`, setting `onboarded`.
- Shown only when `onboarded` is false and no profiles exist, so existing installs skip it.
- Ran: aegis-desktop tests (7 pass, 3 new), aegis-core tests (69 pass), clippy clean, tsc clean, vitest 30 pass, release app built.
- NOT verified: the screens in the running app; Proton link URLs (account.proton.me paths are from memory, check each opens the right page); macOS may ask permission to read Downloads on first scan.

## 2026-11-18: leak test, exit IP, daemon update (macOS, live)

User connected to JP-FREE#2 on the rebuilt app and sent a screenshot (their report, not a scripted run):
- Leak test panel: "No leaks found". Kill switch, DNS, IPv6, reachability and public IP all Protected. Public IP check showed egress IPv4 37.19.205.194; the connection details showed it as Japan (server's listed country).
- Kill switch, DNS and IPv6 protection all read Verified while connected; handshake and traffic counters were live.
- Found and fixed: the installed root daemon was older than the app, so connecting failed with "malformed request" and the screen stuck on "Preparing". Reinstalling the daemon (`install-daemon.sh install <daemon> <wireguard-go>`) fixed it. wireguard-go is not in Homebrew on this Mac, so the installer needs it passed explicitly.
- Found and fixed: a failed connection setup (bypass or firewall step) left the state at Preparing, and Cancel did nothing. The engine now reports Failed, and Cancel does a full disconnect. aegis-core 71 tests pass, tsc clean.
- Not yet verified live: Skip-the-VPN bypass behaviour, tray Quick connect, the fixed Cancel button (the stuck state was not reproduced after the daemon update), a leak test with a deliberately leaking state.

## 2026-11-18: Skip-the-VPN bypass list (macOS, live)

Added 1.1.1.1 to the bypass list, reconnected to JP-FREE#2, then checked the routing table from the shell:
- `route get 1.1.1.1`: gateway 192.168.10.1 via en0 (bypassed, as intended). Host route `1.1.1.1/32 -> 192.168.10.1 (en0)` present.
- `route get 8.8.8.8`: default via utun8 (still tunnelled).
- Full-tunnel split routes 0/1 and 128.0/1 on utun8 remained in place; a plain HTTP request to 1.1.1.1 still answered (301).
- Verified: IPv4 route behaviour for a single address. Not verified: CIDR ranges, DNS for bypassed names (still goes through the VPN DNS), bypass entries being removed on disconnect, a bypassed LAN device such as the Hermes host.

## 2026-11-18: Trusted networks (macOS, built, not yet live-tested)

- Network identity is the router's hardware address (`route -n get default -ifscope en0`, then `arp -n <gateway>`). On this Mac `networksetup`, `ipconfig getsummary` and `system_profiler` all return the Wi-Fi name as `<redacted>`, so SSID matching is not possible without a location permission.
- Unit tests pass for arp/route parsing, MAC normalisation, and the decision table (trusted -> disconnect, untrusted idle -> connect, offline/disabled -> nothing). Clippy clean, UI tests 30 pass, release app rebuilt.
- Not tested live: joining a trusted vs untrusted network with the feature on, the scoped route lookup while the VPN is connected, Ethernet, a router that does not answer ARP.
- `cargo test --workspace` on macOS also runs the Linux-only `aegis-helper` tests; 3 of them fail here (not touched by this change).

## 2026-11-18: DNS firewall rule (macOS), live check

With the VPN connected and Allow local network on: `dig @192.168.10.1 example.com`
timed out (blocked); default lookups resolved via the tunnel DNS (10.2.0.1);
`curl https://example.com` returned 200; ping to the router succeeded. Not
covered: DNS/IPv6 leaks with the kill switch off, DNS-over-HTTPS in browsers.
Unit test `dns_outside_the_tunnel_is_blocked_even_with_local_network_allowed` passes.
