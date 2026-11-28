# Remediation report

## 1. Executive summary

Aegis entered the audit with a working Linux NetworkManager backend, a
session-scoped Linux firewall helper, a desktop UI and an unfinished Windows
pipe client. The baseline working tree already held helper hardening and
tests. The audit preserved it and addressed the credential boundary, firewall
status, DNS probe routing, profile persistence and desktop integration.

## 2. Findings

The [gap register](FULL_GAP_AUDIT.md) tracks 21 findings: 3 P0, 7 P1, 10 P2 and
1 P3. A mitigation that disables an unfinished feature is listed separately
from its implementation and platform acceptance.

## 3. Changes

- A01: Windows production selection returns Unavailable before opening an
  unauthenticated pipe. The portable protocol and Windows compile checks stay.
- A02: protected activation checks kernel state before and after backend
  activation. Monitoring revokes verification on a missing or unreadable
  block. A confirmed absence stops the tunnel immediately. An unreadable block
  gets one bounded retry with the installed rules left in place.
- A03/A08: Linux binds each DNS probe to the owned tunnel device and a tunnel
  source address. DNS verification checks the live resolved link's servers.
  The IPv6 fallback applies only to the intended IPv4-only profile case.
- A09/A10/A13: simulation uses isolated temporary metadata and in-memory
  credentials. Startup leaves untracked backend configurations intact.
  Profile edits restore the previous backend config after metadata write
  failure. Atomic writes use distinct temporary files.
- A11/A12: the tray exposes retained-block removal and respects preferred
  profile eligibility. Rust publishes authoritative settings changes.
- A20: NetworkManager caches handshake samples by activation and interface.
  Rust and the Dashboard distinguish a handshake from received tunnel data.
- A15/A19: the manual Linux preview workflow requires installer outputs and
  uploads verified checksums and Rust and frontend dependency inventories. The desktop
  library uses the Rust library type it needs, avoiding extra link outputs.
- A16: background network observations release the connection lock so a
  slow DNS check cannot block a server switch. Results are discarded when
  the session, network path, or permission to probe changes. Location edits
  wait for the current operation and are validated against the resulting state.
- A21: installers include the project and redistributed asset notices.
- A02 follow-up: the helper compares complete kernel JSON policy against a
  root-owned snapshot. Changed expressions, rule order or rules revoke
  verification. Existing installations without a snapshot must reconnect.
- A06 follow-up: Linux build hooks compile the helper for the package target.
  DEB/RPM bundles install it with its Polkit policy and cleanup hooks.
  Upgrade retains the firewall block. Failed cleanup stops package removal.
  Debian VM install, reinstall, failed-cleanup retention, recovery and purge
  passed. The live IPv4-only policy suite passed all 118 assertions.

## 4. Remaining issues

See [REMAINING_GAPS.md](REMAINING_GAPS.md) for priorities and ownership. Required
external work includes the Windows service, protected switching, Always On,
helper packaging and clean lifecycle tests, and complete native evidence.

## 5. Platform status

- **Linux:** NetworkManager path with Debian 13 VM coverage. Unsigned debug
  amd64 DEB and x86-64 RPM artifacts were built, inspected and checksummed.
  Desktop DEB clean installation, same-version reinstall and purge passed
  in a Debian VM. Distinct-version upgrade, RPM installation, desktop-native
  testing, Ubuntu, Fedora and physical suspend/roaming remain unverified.
- **Windows:** MSVC-target compile check and portable protocol tests. Production
  VPN backend unavailable pending an authenticated service and native validation.

## 6. VPN functionality

| Capability | Debian 13 VM evidence |
|---|---|
| Configuration and tunnel creation | Native WireGuard configuration import and NetworkManager activation passed. The controlled peer was in a guest network namespace. |
| Handshake and traffic | Real recent handshake and sentinel replies passed. Separate peer silence and recovery were exercised. |
| IPv4 routing | Full-tunnel sentinel and default route used WireGuard. The WireGuard UDP endpoint remained reachable through its transport route. |
| IPv6 routing | Dual-stack tunnel passed a real IPv6 sentinel probe. The IPv4-only profile sent IPv6 into the tunnel to be dropped. Captures checked for physical-interface bypass. |
| DNS | Resolver answers matched the tunnel server. Ordinary DNS uplink captures showed zero packets after a positive control. A silent tunnel DNS server failed the reachability check. |
| Kill switch | Uplink captures confirmed ordinary traffic blocking and retained rules after tunnel loss. Deliberate removal revoked verification and stopped the tunnel. The tested protection is session-scoped. |
| Country switching | Controlled country-labelled profiles exercised unprotected switching and active-profile updates. Protected switching remains refused. |
| Reconnection and cleanup | External tunnel loss, simulated upstream link change and NetworkManager restart were exercised. Cleanup compared routes, rules, DNS and nftables against the guest baseline. |

See [TEST_RESULTS.md](../testing/TEST_RESULTS.md) for the assertions and software
snapshot involved in each run.

## 7. Security results

Source review, deterministic regressions and isolated traffic tests are
recorded separately in [TEST_RESULTS.md](../testing/TEST_RESULTS.md).
No independent formal audit was conducted. Firewall polling detects loss
after it occurs. It cannot prevent or reverse administrator rule changes.
The release checklist requires direct verification of the claimed policy.

## 8. Performance

Both debug binaries reached a visible window in about 1.01 s at the median
under Xvfb and mock mode. The full launch wrapper averaged 1.310 s before and
1.214 s after, including display and session setup. One idle main-process
sample gave 276 MiB and 0.00% CPU before, 275 MiB and 0.50% CPU after. These
samples exclude the WebKit child processes and real connected workloads.
The pre-audit binary has no recorded revision. See the measured commands,
timing spread and failed attempts in [TEST_RESULTS.md](../testing/TEST_RESULTS.md).

## 9. Build and test commands

See [TEST_RESULTS.md](../testing/TEST_RESULTS.md) for the exact commands,
counts, versions, advisory database revision and failures.

## 10. Release classification

**Development Preview.** Local regression checks and controlled Debian
network evidence support preview use. The required Windows, protected
transition and installation matrix remains open. Preview artifacts are
unsigned and publication remains a manual decision.

## 11. Remaining manual tests

Run the [native matrix](../testing/TEST_PLAN.md#release-evidence-matrix) with a
controlled VM console and, where required, administrator authorization and an
operator-controlled WireGuard peer. Include install, upgrade, uninstall, real
suspend, reboot, network changes and crash recovery.

## 12. Priority order

1. Validate or strengthen retained firewall policy identity and continuous
   protected transitions before claiming coverage for those modes.
2. Implement and verify the authenticated Windows service and installer.
3. Package the Linux helper safely and exercise clean lifecycle on Debian,
   Ubuntu and Fedora.
4. Retest unavailable native scenarios and record representative desktop
   resource measurements before promoting the preview.
