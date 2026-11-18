# Aegis VPN security review

Date: 2026-11-16  
Revision reviewed: `b93f017270cb58b9f2da64e0086a7260ab2bf239`

## Linux helper follow-up — 2026-11-17

Reviewed the privileged helper and its installer against `548523e`, including
native callers, Polkit actions and recovery/error handling. Source fixes are
implemented. On 2026-11-18, the updated helper and policy ran in the Debian VM.
Both tunnel modes recovered the firewall block from the kernel after a drop and
confirmed its removal after Disconnect. The guest used a test Polkit rule.
Desktop authentication prompts remain untested.

| ID | Severity | Finding and fix |
|---|---|---|
| 11 | High | Caller umask could produce a writable root runtime directory; marker writes followed links. Removed marker writes; the directory is owned/permission-checked, lock opens reject symlinks, nonregular files and multiple hard links. |
| 12 | Medium | Separate helper invocations could interleave firewall changes and state publication. A persistent kernel file lock now covers each mutation; kernel inspection replaces marker publication. |
| 13 | Medium | Password-free firewall changes affected every local user, contrary to the documented user-only scope. Per the user's decision, apply/remove now require administrator authentication; read-only status/handshake have separate argv1-scoped actions. |
| 14 | Medium | Uninstall ignored failed cleanup and could remove the only cleanup tool. It now aborts and retains the helper/policy; a successful list-tables query distinguishes absence from command errors. |
| 15 | Medium | Readback failure could remove a previous fail-closed block; interruptions could leave a block without a marker. Readback failures preserve rules; startup and failed operations inspect the kernel, including blocks without markers. |
| 16 | Medium | Inspection errors became false absence, and a post-removal marker error left a false Verified claim. Backend `Result<bool>` preserves errors; core records unknown state, rechecks partial failures and keeps Disconnect retryable. |
| 17 | Low | nft diagnostics could echo imported endpoint addresses through IPC. The helper reports operation status and remediation without returning the rule body. |

Implementation: `services/linux-helper/src/main.rs:184` (inspection), `:241`
(descriptor checks), `:261` (lock), `:288` (apply);
`services/linux-helper/org.aegisvpn.helper.policy:9` (authorization);
`scripts/install-helper.sh:22` (uninstall gate);
`crates/aegis-core/src/engine.rs:49` (unknown firewall state).

Validation: helper filesystem/lock and injected-nft tests, core failure tests,
wrapper propagation, policy XML checks, uninstall stub checks and the null-profile
Disconnect UI regression. The cleanup-after-removal regression failed before
the fix and passed afterward. Final commands/results are recorded in
[TEST_RESULTS.md](docs/testing/TEST_RESULTS.md).

Upgrade both the helper binary and policy together using the existing installer.
The new app expects `status`; an older helper leaves inspection explicitly
unknown. Custom Polkit rules referencing the old action ID must be updated to
the separate apply/remove IDs. See [README.md](README.md#linux-helper-install-or-upgrade).
The Polkit first-argument scoping follows its
[official pkexec documentation](https://www.freedesktop.org/software/polkit/docs/latest/pkexec.1.html);
kernel locking follows [flock(2)](https://man7.org/linux/man-pages/man2/flock.2.html).

## Integration follow-up — 2026-11-17

The later kill-switch integration added two gaps in the core engine: cancelling
a reconnect removed the block even after failed teardown, and recovery could
adopt/reactivate a tunnel based on helper support without an installed block.
Both were reproduced with failing regression tests and fixed. Cancellation
now preserves the block on teardown failure; activation, startup adoption and
status reconciliation require an installed block when the kill switch is
requested. An existing unprotected tunnel remains `Uncertain` until the user
disconnects; a subsequent Connect installs the block normally.

Combined working-tree validation passed 100 Rust, 27 frontend and two optimized
GLib tests, Clippy, TypeScript, formatting and frontend lint. A new VM run was attempted
but could not start because `/dev/kvm` is absent; earlier live results below
remain scoped to their recorded snapshots. See
[TEST_RESULTS.md](docs/testing/TEST_RESULTS.md). This follow-up leaves the
other session's IPv6 implementation untouched and does not independently
audit the new privileged helper. GLib and library-buffer limits below remain.

## Implementation status — 2026-11-16

The findings below describe the original audit snapshot. The working tree now
contains these remediations. Per the user's scope decision, automatic IPv6
blocking belongs to the other session. Final audit-only validation used the
frozen `/tmp/aegis-audit-validation` snapshot, excluding that session's IPv6
blocking, runtime verification, server registry and UI redesign work.

| IDs | Remediation |
|---|---|
| 1–2 | Validate default-route/DNS address families; cached protection reporting requires matching addresses and real resolver IPs, not search domains. |
| 3 | Check current requested protection before every activation; reject enabling unavailable protection during a session; surface unmet recovery policy without silently dropping a live tunnel. |
| 4–5 | Non-idle `Uncertain` state invalidates protection claims, keeps Disconnect retryable and blocks edits/deletion; teardown errors abort switching. Successful status observations can restore the session without activation. |
| 6 | Runtime directory stays root-owned; cleanup uses kernel namespace PIDs, never writable PID files. |
| 7 | Pin the reviewed compatible GLib backport, guarded by source-pin and optimized iterator tests. See [GLIB_BACKPORT.md](docs/security/GLIB_BACKPORT.md). |
| 8 | Capture and snapshot commands must succeed; snapshots preserve order and duplicates; no broad firewall XFAIL suppression remains. |
| 9 | Decode keys from borrowed D-Bus strings rather than additional ordinary owned strings. Library serialization buffers still cannot be guaranteed wiped. |
| 10 | Open once, validate descriptor metadata, bound reads to 64 KiB + 1 in zeroizing storage; Unix imports reject symlinks and nonregular files without blocking on FIFOs. |

Final scoped validation: 67 Rust tests, 10 frontend tests, two optimized GLib
tests and 91 live disposable-VM checks passed. Clippy, TypeScript, formatting,
frontend lint and shell checks passed. See [TEST_RESULTS.md](docs/testing/TEST_RESULTS.md).
New config/engine regressions were observed failing before their production fixes.
An intermediate VM run exposed stale NetworkManager active-object paths; the
final fix reconciles them against a fresh ObjectManager snapshot and passed
all restart/cleanup cycles. These results do not validate the combined,
concurrently changing working tree.

The audit-only patch is `/tmp/aegis-audit-validation/audit-fixes.patch` (27 files,
checked to apply to the audited revision and reproduce the tested files).
Changes remain uncommitted.

Remaining limits: GLib 0.18 is EOL and its backport is unmerged upstream;
four transitive maintenance warnings remain; D-Bus buffer wiping is outside the
current library API. Windows runtime/ACL and live webview penetration tests
remain unperformed. The absent kill switch/helper are still unsupported.

## Executive summary (original audit)

The review found **3 high, 5 medium and 2 low severity issues**. The highest
priorities are inaccurate IPv6/DNS protection reporting and inconsistent
enforcement of the requested kill switch. Eight isolated probes reproduced
failure cases across five findings. Passing these probes confirms the defects;
it does not mean those security properties passed.

No critical vulnerability, direct profile-script execution, frontend HTML
injection, or normal-path IPC disclosure of private keys was identified in the
reviewed code. This is a source review with targeted local testing, not a
penetration-test certification or proof that other vulnerabilities do not exist.

| ID | Severity | Finding | Evidence |
|---|---|---|---|
| 1 | High | IPv6 protection can be reported when IPv6 is disabled on the tunnel | Mapping probe + source trace |
| 2 | High | DNS protection can be reported when no DNS servers are installed | Two mapping probes + source trace |
| 3 | High | Reconnect bypasses the requested-protection check | Mock reconnect and recovery probes |
| 4 | Medium | Status failures leave a disappeared tunnel reported as connected | Fault-injection probe |
| 5 | Medium | Failed disconnect returns success and can later become false Disconnected | Two fault-injection probes |
| 6 | Medium | Root-run test cleanup trusts a user-writable PID file | Source review; developer tooling only |
| 7 | Medium | Linux desktop includes an unsound GLib dependency | Fresh advisory check + dependency tree |
| 8 | Medium | VM security checks can hide capture failures and firewall changes | Source review; test assurance |
| 9 | Low | D-Bus conversions leave ordinary heap copies of secret keys | Source review; defense in depth |
| 10 | Low | Import reads are not bounded at the open file descriptor | Source review; availability hardening |

## Scope and method

Reviewed the Rust configuration parser, key handling, core engine, profile
storage, diagnostics, Linux NetworkManager backend, Windows stub, Tauri command
handlers/capabilities/CSP, React/TypeScript UI, shell/VM tooling, dependency
lockfiles, and CI/release configuration. The application has no web backend,
account/session system, or implemented privileged helper/service to audit.

The requested `security-best-practices` skill supplies React/JavaScript guidance.
It does **not** supply Rust, Tauri, NetworkManager or Bash-specific reference
documents. Those parts were reviewed directly against the project's trust
boundaries and primary platform documentation. Web-server cookie/CSRF/HSTS rules
were not applied to this local desktop application.

Validation performed during this review:

- `cargo test --workspace`: **51 passed**.
- `node node_modules/vitest/vitest.mjs run` in `apps/desktop`: **9 passed**.
- An isolated Cargo test project at `/tmp/aegis-security-probe`: **8 passed**,
  each asserting an observed failure case. Only in-memory mock backends and pure
  settings conversions were used; no host networking was changed.
- `cargo audit -d /tmp/aegis-security-advisory-db --json --no-yanked`: **0
  classified vulnerabilities**, **1 unsoundness warning**, **4 unmaintained
  package warnings**. Database commit `02e2517f457a359fc6d3440ea4560c8ea188fee8`,
  updated 2026-11-15; lockfile contains 512 packages. The separate yanked-release
  check was explicitly disabled, so this is not a yanked-package clearance.
- `sh /home/mikeh/.npm-global/bin/pnpm audit --json` in `apps/desktop`: **0
  advisories**, including development dependencies.
- `cargo tree -i glib@0.18.5 --depth 3`: confirms the GTK/Tauri dependency path.

The normal Cargo advisory-cache location was read-only, so a fresh database was
used under `/tmp`. The installed pnpm launcher requires a shell; invoking it
through `sh` completed the audit. Neither workaround changed project files.

## High severity — fix before presenting these protections as configured

### 1. IPv6 protection can be reported while the tunnel disables IPv6

**Rule:** Report protection from the configuration actually applied, not merely
from requested routes.

**Locations:** `crates/aegis-config/src/lib.rs:178`,
`crates/aegis-config/src/validate.rs:14` and `:84`,
`crates/aegis-linux/src/settings.rs:142`,
`crates/aegis-linux/src/nm.rs:344` and `:355`.

**Evidence:** Route classification examines `AllowedIPs`. A profile with an IPv4
tunnel address and both `0.0.0.0/0, ::/0` is valid, classified `Full`, and can
have no warnings. However, `to_settings` returns `method = "disabled"` for any
address family with no tunnel address. `NmBackend::protection` unconditionally
returns `Configured("IPv6 default route goes through the tunnel")` for `Full`.

**Impact:** On a dual-stack host, IPv6 can remain on the ordinary uplink while
the app reports tunnel IPv6 routing as configured. An incorrectly configured
provider profile is enough to trigger the mismatch; no frontend compromise is
required. Actual packet leakage for this malformed profile was not live-tested.

**Reproduction:** Remove the IPv6 address and IPv6 DNS server from the full
fixture while keeping its IPv6 default `AllowedIPs`. The isolated probe confirms
validation succeeds, warnings are empty, classification is `Full`, and the
generated IPv6 method is `disabled`.

**Suggested fix:** Reject default routes for an address family the backend will
disable, or explicitly report that family as `Failed` and require informed
acceptance of the limitation. Compute protection from effective per-family
settings. Preserve supported IPv4-only profiles and their explicit leak warning.
Add IPv4-only, IPv6-only and mixed-family regression cases; verify malformed
profiles in the disposable dual-stack VM before claiming runtime protection.

### 2. DNS protection can be reported without any installed DNS servers

**Rule:** Search domains and unusable DNS addresses are not evidence of an
installed resolver.

**Locations:** `crates/aegis-core/src/types.rs:61`,
`crates/aegis-linux/src/settings.rs:124`, `:142` and `:168`,
`crates/aegis-linux/src/nm.rs:338` and `:347`.

**Evidence:** `ProfileSummary::from_config` combines DNS server addresses and
search domains in `summary.dns`. The backend then uses only
`!profile.dns.is_empty()` to decide DNS protection is configured. A profile whose
only DNS entry is `corp.example` has no resolver addresses but satisfies that
test. Separately, a DNS server belonging to a disabled address family is dropped
by `to_settings`, while it remains in the summary.

**Impact:** DNS queries can continue to use local-network resolvers while the
dashboard says the tunnel DNS is configured. The search-only case also has a
profile warning about missing servers, producing contradictory UI signals.

**Reproduction:** Both isolated probes succeed: (a) clear DNS server addresses
and retain a search domain, or (b) use only an IPv4 tunnel address and an IPv6
DNS server. Both summaries contain DNS entries, but neither generated IP section
contains a `dns` property. The latter case also has no profile warning.

**Suggested fix:** Distinguish resolver addresses from search domains and test
whether at least one resolver is actually installed on an enabled family. Derive
the status from those effective settings; return `Failed` when none is usable.
Do not promote configured DNS to verified without checking resolver integration.
NetworkManager's exclusivity behavior also depends on other active DNS
priorities; a fixed `-50` does not guarantee it wins over every other profile.
See the [NetworkManager DNS priority reference](https://www.networkmanager.dev/docs/api/latest/settings-ipv6.html).

### 3. Requested kill-switch enforcement is bypassed by reconnect

**Rule:** Every activation path must enforce current requested protections.

**Locations:** `crates/aegis-core/src/engine.rs:360`, `:377`, `:396`, `:406`,
`:588`, `:620` and `:659`; `apps/desktop/src/screens/Settings.tsx:109`.

**Evidence:** Initial `connect()` checks unsupported requested protection, but
`reconnect_loop()` calls `activate_once()` directly. Settings can enable the
unsupported kill switch while a tunnel is active. Recovery also adopts an active
tunnel without checking the persisted request, and the same-profile early return
precedes the protection check.

**Impact:** After the user enables a protection that the application promises
will prevent unprotected connections, a dropped tunnel can reconnect without
that protection. This is distinct from the already documented absence of a kill
switch in this version; the defect is bypassing its explicit fail-closed policy.

**Reproduction:** Connect the mock backend, enable `kill_switch`, drop its tunnel,
and advance the monitor/retry clock. The engine returns to `Connected` despite
unsupported capability. A second probe shows startup adopts an already active
mock tunnel with the persisted request enabled.

**Suggested fix:** Put the current protection check immediately before every
activation, including retries, and serialize relevant settings changes with
connection operations. Reject enabling unsupported protection on an active
session, or use an explicit policy transition that does not leave a misleading
enabled setting. Recovery should surface the unmet requirement; do not silently
tear down a persisted tunnel without considering the traffic-leak implications.
Add tests for initial connect, settings changes during connect/retry, reconnect,
same-profile connect, and recovery. The mock reproduction proves the shared
engine control-flow defect; the real Linux failure transition needs VM testing.

## Medium severity

### 4. Backend status errors preserve stale Connected state indefinitely

**Locations:** `crates/aegis-core/src/engine.rs:518`, `:539` and `:583`,
`apps/desktop/src/screens/Dashboard.tsx:98`.

**Evidence:** `Err(e)` from the monitor's `backend.status()` only logs a warning.
It does not invalidate the last connected state, stats or derived protection.
There is no freshness timeout. Peer confirmation also uses cumulative received
bytes, which proves an earlier response rather than continuing reachability.

**Impact:** If the tunnel disappears while D-Bus/status queries fail, the UI can
keep showing an active interface and configured protections. Without a kill
switch, the host may meanwhile route traffic normally. A historical server
response must not be presented as proof of current protection.

**Reproduction:** A mock wrapper drops the active tunnel and makes status calls
fail. After several polling intervals the backend has no tunnel, the log has
status warnings, and the engine still reports `Connected`.

**Suggested fix:** Track the age of the last successful observation; after a
bounded interval report status/protection as unknown or failed, expose that in
the UI, and attempt reconciliation. Do not infer disconnection or remove
network state solely from a telemetry failure. Keep the historical response
label explicit. Test transient failures and sustained loss separately.

### 5. Deactivation failures are swallowed and permit unsafe state transitions

**Locations:** `crates/aegis-core/src/engine.rs:408`, `:478`, `:484` and `:492`;
`crates/aegis-core/src/types.rs:222`.

**Evidence:** `teardown()` returns `()`, sets `Failed` on backend failure, and
`disconnect()` still returns `Ok(())`. Because `Failed` is considered idle, a
second disconnect sets `Disconnected` without retrying the backend. Profile
switching also ignores failed teardown and activates the replacement profile.

**Impact:** A tunnel can remain active after a successful-looking disconnect.
Switching profiles can leave the previous tunnel/routes/DNS active alongside the
new session, causing unexpected routing or use of the wrong VPN identity.

**Reproduction:** One fault-injection probe confirms two disconnect calls return
success and the UI state becomes `Disconnected` while the backend remains
active. Another confirms replacement activation proceeds after deactivation
failure. The mock does not model simultaneous real NetworkManager tunnels, so
the exact routing consequences are inferred rather than packet-tested.

**Suggested fix:** Return and propagate `Result` from teardown, abort a profile
switch when removal fails, and retain an actionable possibly-active state.
Retry/reconcile a failed teardown instead of treating every failure as idle.
Extend existing profile-switch/disconnect tests with deactivation failures.

### 6. Privileged peer-test cleanup trusts a user-writable PID file

**Locations:** `scripts/test-netns-peer.sh:35`, `:125` and `:130`.

**Evidence:** Successful setup changes ownership of `/run/aegis-test` to the
invoking user. Later root-run cleanup reads `dnsmasq.pid` from that directory
and runs `kill` on its contents. Directory ownership lets that user replace a
root-owned child file even if the child itself is not writable.

**Impact:** A compromised invoking account can supply an unrelated PID before
an administrator authorizes cleanup, causing the script to signal an arbitrary
root-owned process. This affects development/test tooling, not the shipped
unprivileged application. It requires a privileged invocation of the script;
there is no demonstrated standalone privilege escalation or remote path.

**Suggested fix:** Keep runtime state and PID files in a root-owned directory
that the caller cannot modify. Give the caller access only to the exported
throwaway profile, with appropriate traversal permissions or a separate export
location. Confirm process identity before signaling, and reject unexpected
existing runtime state/symlinks. Test using harmless processes in a disposable
VM. No privileged exploitation was attempted during this review.

### 7. GLib 0.18.5 has a known unsound iterator implementation

**Locations:** `Cargo.lock:1482`, `apps/desktop/src-tauri/Cargo.toml:21`.

**Evidence:** Fresh Cargo audit output flags `glib 0.18.5` with
`RUSTSEC-2024-0429`. The dependency tree includes GLib through GTK, WebKit and
the Tauri desktop stack. The advisory describes undefined behavior and possible
optimized-build null-pointer crashes in `VariantStrIter`; patched versions are
`>=0.20.0`. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2024-0429.html).

**Impact/qualification:** The affected package is present; reachability of the
affected iterator from hostile input in Aegis was not demonstrated. No direct
application call was found. This is a dependency risk, not a confirmed Aegis
memory-corruption exploit, and Cargo categorizes it as an informational
unsoundness warning rather than one of its classified vulnerabilities.

**Suggested fix:** Prefer a compatible desktop dependency update that removes
the affected GLib branch. If that is unavailable, evaluate a narrowly scoped
upstream fix backport and validate GTK/Tauri integration. Do not force an
incompatible GLib major version into the graph. Record a bounded exception with
owner/review date if release must proceed, and cover optimized desktop builds.

Four additional maintenance warnings are `proc-macro-error 1.0.4`
(`RUSTSEC-2024-0370`), `zbus_names 4.3.5` (`RUSTSEC-2026-0371`),
`zvariant 5.15.1` (`RUSTSEC-2026-0370`) and `zvariant_derive 5.15.1`
(`RUSTSEC-2026-0372`). Maintenance warnings do not themselves prove
vulnerabilities. Track compatible upstream replacements.

### 8. VM security tests can misclassify verification failures as success

**Locations:** `crates/aegis-linux/examples/vm_suite.rs:54`, `:89`, `:105`,
`:125`, `:127` and `:404`.

**Evidence:** `dns_probe` ignores capture termination/read exit status, combines
stderr/stdout, and counts matching packet lines. An unreadable or failed capture
can therefore produce `0` packets, which is later accepted as no leak. The
earlier positive control does not prove a later capture succeeded. Separately,
`diff_lines` uses membership rather than multiplicity, so duplicate firewall
rules can be invisible. The XFAIL classifier permits generic added table/chain
headers and mark rules without tying them to the known NM leak.

**Impact:** A broken verification setup or an unrelated firewall regression can
receive a clean or expected-failure result. This does not establish an app
traffic leak, but weakens the security claims based on the suite, especially its
intended detection of duplicate NetworkManager rules.

**Suggested fix:** Require a successfully started capture, successful stop/read,
and a readable valid pcap before accepting a zero count; otherwise fail or mark
the check inconclusive. Check snapshot commands' success. Compare firewall
rules with multiplicities/structure and narrowly identify the known leak by
owned interface/address/table. Preserve D11: detect/report NM-owned leftovers,
never remove them. Add failure-injection tests for capture errors, duplicate
rules and unrelated added tables. Findings here are from source inspection;
the previous documented 89-PASS VM run was not repeated in this audit.

## Low severity / defense in depth

### 9. NetworkManager serialization creates non-zeroizing secret copies

**Locations:** `crates/aegis-linux/src/settings.rs:89`, `:99`, `:183`, `:208`
and `:274`; `crates/aegis-linux/src/nm.rs:252`.

**Evidence:** Zeroizing base64 values are copied with `.to_owned()` into
ordinary D-Bus variants, and loading reconstructs ordinary `String` copies
before parsing `SecretKey`. Their allocations are not wiped by the
`SecretKey` destructor. D-Bus serialization can make further copies.

**Impact:** Freed heap memory or a process dump can retain key material beyond
the lifetime of the protected key type. No cross-user read, crash dump exposure,
or actual key extraction was demonstrated. This is outside a remote attacker's
normal UI access and should not be described as an IPC key disclosure.

**Suggested fix:** Wrap application-owned decoded strings in `Zeroizing` as
early as possible, minimize variant lifetime and cloning, and document the
remaining library-buffer limitation. Consider disabling core dumps for release
processes containing keys. Avoid claiming that every temporary secret copy is
zeroized; keep the existing redacted types and secret-free IPC tests.

### 10. File imports check metadata before an unbounded pathname read

**Location:** `crates/aegis-core/src/engine.rs:239`.

**Evidence:** Import calls `metadata(path)`, validates file type/length, then
separately calls `read_to_string(path)`. A replaced or growing file can bypass
the pre-read size/type check. The parser's later 64-KiB check still rejects the
result, but only after the read/allocation finishes.

**Impact:** A user importing a concurrently modified/shared file can suffer
excessive memory use or a blocking read. Paths originate from native selection
or OS drag/drop, which substantially limits exposure. This is not remote
arbitrary-file access through IPC, and no resource-exhaustion attack was run.

**Suggested fix:** Open once, inspect metadata on that descriptor, read at most
`MAX_CONFIG_BYTES + 1`, reject overflow, and then decode/parse in zeroizing
storage. Define symlink handling explicitly; avoid blocking special-file opens.
Test oversized/growing input and nonregular-file rejection.

## Controls that were present

- The parser rejects executable lifecycle hooks, unsupported routing directives
  and unknown keys. Endpoint/domain validation and ordinary parse errors do not
  interpolate supplied field values.
- Private key types have redacted `Debug`, no `Serialize`/`Display`, OS-backed
  generation and zeroization. Normal profile/draft/snapshot/event/diagnostic
  secret-exclusion tests pass.
- Tauri uses a production CSP without `unsafe-eval`/`unsafe-inline`, prototype
  freezing, an explicit command manifest, and local main-window capabilities
  without shell/filesystem/network plugin access. Paths come from native dialogs
  or OS drops. This matches the [Tauri capability model](https://v2.tauri.app/security/capabilities/).
- React uses escaped text rendering; the reviewed frontend has no raw HTML
  insertion, dynamic code evaluation, credential storage, remote scripts or
  dynamic navigation sinks. Vite binds development service to loopback and
  disables production source maps.
- Unix atomic output creates new mode-0600 temporary files. Connection changes
  verify NetworkManager type, UUID and derived Aegis interface name. The app
  operates unprivileged; backend secrets are delegated to NetworkManager.
- Mock mode is explicit and labelled. Windows networking and automatic updates
  report unavailable rather than silently simulating production protection.
- CI actions are pinned to commit SHAs and use read-only repository permissions.
  Lockfiles and restricted Cargo dependency sources are present.

## Suggested implementation order

1. Correct effective IPv6/DNS configuration and reporting (#1–#2), preserving
   honest support for intentionally partial tunnels.
2. Enforce current requested protections across connection paths (#3), then
   correct teardown and stale-state handling (#5, #4).
3. Tighten the VM checks (#8) before using them to validate fixes; repair the
   privileged test PID handling (#6).
4. Resolve or explicitly track the GLib advisory (#7); make the small secret
   lifetime and bounded-read improvements (#9–#10).

## Limits and release gates

No implementation fixes, commits or pushes were made by this audit. Gitleaks
was not run, following the explicit preference against unrequested scans.
The presence of public test vectors is not reported as a credential leak.

The existing full/full4 VM results were read as historical evidence, not
reproduced. This audit did not alter host routing, restart NetworkManager, run
root exploit tests, inspect process memory, test Windows ACLs/installers,
penetration-test the live webview/CSP, or verify GitHub branch/security settings.
System WebKit/GTK/NetworkManager package vulnerabilities are outside Cargo/npm
lockfile coverage. No independent WireGuard cryptographic audit was performed.

The absent kill switch and Windows service, unsigned release artifacts, and the
planned NM firewall-leak warning remain documented release constraints. Do not
present those planned features as implemented. Windows private exports will need
explicit destination-ACL validation before real secrets are supported there;
the current non-Unix writer relies on inherited permissions, including for
user-selected export locations.

The eight local probes can be rerun while their temporary project exists:

```sh
rtk proxy env CARGO_TARGET_DIR=/home/mikeh/Projects/aegis-vpn/target cargo test --offline --manifest-path /tmp/aegis-security-probe/Cargo.toml
```

Move the relevant cases into permanent regression tests when fixing each
finding. Until then, these probes assert the defective behavior deliberately.
