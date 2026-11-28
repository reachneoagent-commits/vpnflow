# VPNFlow gap audit

Baseline: main at 548523e, with existing worktree changes to the helper,
Windows adapter, workflows, VM harness and documentation. Those changes form
part of the baseline. Audit edits remain in the shared worktree to preserve
that state. Scope covers the Rust crates, desktop surfaces and IPC, the helper,
storage, network policies, packaging, workflows and release documentation.

Baseline formatting, locked offline workspace check, Clippy and all-target
tests passed (131 Rust tests). Frontend lint, types, 29 tests and build passed.
Local RustSec audit detected five unmaintained dependencies and no classified
vulnerabilities in its cached database. Cargo-deny initially could not lock
its database in the read-only Cargo cache. The VM harness initially failed
writing its image checksum into the host cache. See TEST_RESULTS for retests.

Severity and platform are explicit below. “Open” means required verification
or functionality is still missing. The release decision is Development Preview.

| ID | Priority / platform | Evidence and affected code | User / security impact | Fix and required verification | Status |
|---|---|---|---|---|---|
| A01 | P0 Windows | aegis-windows/lib.rs: ServiceBackend::connect and call trust a pipe name and version reply; Install serializes the full config | Unauthenticated process can receive credentials or fabricate status | Keep production backend unavailable until an authenticated service exists. Verify constructor rejects before IPC; native hostile-peer test before enabling | Mitigated, MSVC compile verified; native auth still open in A04 |
| A02 | P0 Linux | aegis-core/engine.rs and linux-helper/main.rs originally trusted cached state and a partial chain check | Firewall replacement can leave a session advertised as Verified | Recheck protection and compare the full kernel policy with a root-owned snapshot, ignoring only object handles and nft metadata | Unit tests and live VM policy-change detection passed. Continuous enforcement against administrator changes remains outside polling guarantees |
| A03 | P0 Linux | aegis-linux/nm.rs: check_reachability/check_public_ip choose a route before verify.rs:dns_exchange opens its own unrestricted socket | A route change can send an app-generated DNS probe outside the tunnel | Bind actual datagram socket to the active tunnel interface and validate local tunnel source. Test missing interface/invalid source and the positive/negative VM lookups | Unit regression passed and VM DNS lookups exercised |
| A04 | P1 Windows | No service in Cargo.toml; Windows handoff and NSIS config lack DLL/SCM, ACL, DPAPI, WFP and cleanup | Windows desktop cannot establish a real tunnel; no installed-system protection | Implement authenticated service using official embeddable DLL and verify install/upgrade/uninstall, spoof rejection, Windows 11 routing and packet captures | Open, native environment required |
| A05 | P1 Linux / Windows | Engine::connect_locked refuses protected switches; helper rules only cover current session and reset at reboot | No protected country switch or Always On. Unprotected switching has a traffic gap | Keep protected switch refused. Implement and test continuous policy across transition, failure, crash and reboot before advertising support | Open |
| A06 | P1 Linux | Linux bundles originally omitted the helper and its policy | Protected mode requires a manual install, and uninstall can strand a firewall block | Bundle the helper for the selected target and the matching policy, declare dependencies, retain the block during upgrade and fail removal if cleanup fails | DEB/RPM built and inspected with helper and hooks. Native lifecycle outcomes are in TEST_RESULTS. Distinct-version upgrade and native RPM installation remain open |
| A07 | P1 Linux | nm.rs: activate/deactivate use asynchronous D-Bus calls; engine bounds activation future, but system-side request can outlive timeout | Late activation/cleanup can leave state uncertain; external NM restart already has a documented upstream rule issue | Verify races in VM with cancellation during activation and daemon restart; do not touch other firewall owners | Open |
| A08 | P1 Linux | verify.rs: judge_ipv6 uses one destination and a lowest-metric default fallback; judge_dns uses resolver configuration, not packet capture | Policy routing, split DNS, forwarded traffic and browser DNS can differ from the checks | Narrow status claims and test routes plus uplink captures across supported distributions and path changes | Resolver address check and narrower IPv6 verdict tested; native matrix open |
| A09 | P1 Linux | MockBackend persistent .conf files in mock.rs; inspect desktop backend and data-dir setup | Risk that switching between simulated and real modes can cross storage/recovery boundaries | Verify metadata and secret namespaces are separate and avoid plaintext persistence of real keys in mock | Temporary metadata and in-memory configuration wired; portable regressions passed |
| A20 | P1 Linux | aegis-linux/nm.rs: the handshake cache can reuse a sample across active connections; aegis-core/engine.rs supplies synthetic handshake timestamps when only receive counters are available | The UI or server history can attribute an earlier handshake to the selected server or present a received-data observation as a handshake | Scope samples to the NetworkManager activation object and interface. Keep the actual handshake timestamp separate from the response observation and test cache scope | Targeted regression and workspace tests passed |
| A10 | P2 All | store.rs: write_private_atomic reuses a PID-based filename and removes it even if opening failed | Concurrent save/export can delete another writer's temporary file and fail | Use unique temporary path, clean only files created by this call, test concurrent writes and permissions | Updated writer; export, private permissions and storage suites passed |
| A11 | P2 All | tray.rs: build_menu disables Disconnect in Failed despite retained firewall block; preferred action ignores eligibility | User cannot stop blocking from tray; disabled/pending profile appears connectable | Share eligibility and expose Stop blocking, validate menu policy in unit tests | Menu wiring compiled; native tray interaction pending |
| A12 | P2 All | store.ts/useAppStore caches settings from startup/invoke while tray calls Engine::set_settings directly | UI can later persist stale preferred-profile settings | Publish authoritative settings changes and test tray-style event synchronization | Settings event tested in frontend and Rust suite |
| A13 | P2 All | Profile/update/delete span backend and JSON files; registry UI and backend mutations reviewed (engine.rs, registry.rs, Locations.tsx) | A disk write or backend failure can leave summary, profile identifiers and credentials out of sync | Preserve existing credentials on failed edit or deletion, test rollback and document crash consistency limits | Edit and deletion rollback regressions passed; failed rollback and cross-store crash consistency remain open |
| A14 | P2 All | Cargo lock, RustSec and deny policy: known unmaintained transitive crates, prior pinned GLib fix | Maintenance risk and possible future advisories | Recheck advisory/license/source policy and record dependency paths; avoid unjustified suppression | Fresh advisory scan and full deny policy passed with warnings |
| A15 | P2 CI | release.yml accepts tags only, includes Windows NSIS artifact with no functional service; checksum glob can succeed with no installer | Users could mistake a shell package for release-ready VPN; missing installer can go unnoticed | Make packaging explicit preview and verify expected outputs and hashes, leave publication manual | Manual preview workflow and artifact checks prepared; hosted run pending |
| A16 | P2 Desktop/Linux | engine.rs: monitor holds the operation lock through background DNS and route checks; snapshots include health for each profile | A VM metadata edit and country switch returned Busy. Large catalogs can incur repeated snapshot work | Release the lock before read-only checks and discard stale results. Queue a metadata edit behind the current operation and revalidate before writing. Measure large catalogs separately | Slow-probe and queued-edit regressions passed. Final `full4` VM run verified the location edit, switch and active profile update |
| A17 | P2 Native matrix | Prior suites cover Debian and mock; Windows, Ubuntu GNOME, Fedora GNOME, reboot and actual suspend remain outstanding | Build success can be mistaken for platform compatibility | Record versioned compatibility matrix and isolation procedure; run each native case before promotion | Open |
| A19 | P2 Build | desktop Cargo.toml built unused staticlib and cdylib variants; optimized test emitted collisions | Redundant output and possible future Cargo error | Retain the Rust library target required by the desktop bin and rerun optimized test | Optimized GLib test binary passed both tests without the collision warning |
| A21 | P2 Packaging | tauri.conf.json bundled licensed data, art and fonts without separate notices in the installed resource directory | Users of binary packages may be unable to locate redistribution notices | Include project licenses and notices for fonts, icons, flags, React and the country data. Inspect both Linux archives | All six files verified in both archives. Full transitive notice and SBOM review still due before publication |
| A18 | P3 Documentation | README, roadmap, Architecture/Decisions and platform handoffs contain stale helper and Windows assertions | Confusing installation and recovery guidance | Align requested guides and release checklist with actual support; preserve CLAUDE's AGENTS import | Guides and checklist updated |

## Coverage and references

- Config parsing, keys and route derivation: aegis-config and its malformed-input,
  secret-redaction and roundtrip tests. Tauri native import/export, capability and
  CSP review covered commands.rs, build.rs, capabilities/default.json and tauri.conf.json.
  A regression checks that every registered app command has an explicit grant
  scoped to the local main webview.
- Connection lifecycle and data: engine.rs, backend.rs, store.rs, registry.rs,
  diagnostics, Linux NM settings/verification/events and the Linux helper/Polkit.
- Desktop: Dashboard, Locations, Profiles, Activity/diagnostics, Settings,
  About, tray, dialogs, notifications and event subscriptions; frontend tests
  cover behavior with a mock. Accessibility on native webviews remains open.
- Distribution: package manifests, dependency policies, administrator helper
  installation, CI, release workflow, targets and icon/flag/font licenses.
- Current upstream references: [WireGuard embedding](https://www.wireguard.com/embedding/),
  [Windows service](https://github.com/WireGuard/wireguard-windows/tree/master/embeddable-dll-service),
  [Windows Filtering Platform](https://learn.microsoft.com/en-us/windows/win32/fwp/windows-filtering-platform-start-page),
  [NetworkManager](https://networkmanager.dev/docs/api/latest/),
  [Tauri capabilities](https://v2.tauri.app/security/capabilities/),
  [Tauri signed updates](https://v2.tauri.app/plugin/updater/),
  [RustSec](https://rustsec.org/) and [GitHub Actions security](https://docs.github.com/en/actions/how-tos/secure-your-work).
