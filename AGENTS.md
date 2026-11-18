# Agent instructions: Aegis VPN

Durable rules for any coding agent (Codex, Claude Code, others) working in
this repository. `CLAUDE.md` imports this file; edit only this one.

## What this is

Open-source WireGuard desktop client for Linux and Windows 11.
Rust core + Tauri 2 shell + React/TypeScript UI. It reuses official
WireGuard implementations and never invents cryptography.

Read before changing behaviour:
- `docs/architecture/ARCHITECTURE.md`: components and data flow
- `docs/architecture/DECISIONS.md`: why things are the way they are
- `docs/security/THREAT_MODEL.md`: trust boundaries
- `docs/ROADMAP.md`: status checklist (keep it current)
- `docs/testing/TEST_RESULTS.md`: what has actually been run

## Layout

```
crates/aegis-config   wg-quick parser/validator, SecretKey (no Display/Serialize)
crates/aegis-core     Engine (state machine, reconnect, recovery), VpnBackend trait,
                      ProfileDraft, Store (non-secret JSON), registry (location metadata on profiles),
                      diagnostics, MockBackend
crates/aegis-linux    NetworkManager backend over D-Bus (zbus), settings mapping
crates/aegis-windows  Windows pipe protocol and test client. Desktop use awaits server authentication (D16).
apps/desktop          React UI (src/), Tauri shell (src-tauri/)
scripts/              test-netns-peer.sh: isolated WireGuard peer for live tests
services/linux-helper aegis-helper: the only root code (pkexec), kill-switch nftables table
services/windows-service aegis-service: LocalSystem pipe server; today a Skeleton handler (no tunnels yet)
```

## Commands

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/desktop && pnpm install && pnpm lint && pnpm typecheck && pnpm test
AEGIS_BACKEND=mock pnpm tauri dev           # UI with the simulated backend
cargo run -p aegis-linux --example nm_probe  # read-only check against NetworkManager
scripts/vm/run-vm-tests.sh                   # full-tunnel/leak suite in a disposable QEMU VM (no root)
```

`cargo clippy` for the desktop crate needs `apps/desktop/dist/` to exist
(run `pnpm build` once, or create an empty `dist/index.html`).

## Non-negotiable rules

1. **No secrets cross into the webview.** Private/preshared keys stay in Rust
   and the backend store. UI types carry `has_*` booleans only. Key edits are
   Keep/Generate/Remove; real keys arrive only via `.conf` import.
2. **No file paths from web content.** Import/export open native dialogs in
   Rust (`commands.rs`); drag-and-drop paths come from the OS window event.
3. **Never log or serialize key material.** `SecretKey` has a redacted
   `Debug` and no `Display`/`Serialize`. Tests assert UI payloads, diagnostics
   bundles and events contain no keys; keep them passing.
4. **Never execute profile scripts.** `PreUp/PostUp/PreDown/PostDown` and
   `SaveConfig` are rejected. `Table`/`FwMark`/unknown keys are rejected, not
   dropped.
5. **Honest state.** `Connected` = backend link up; peer confirmation is
   `PeerState`; reachability is only the tunnel-DNS lookup (DECISIONS D13). Protection features are
   `Off/Unsupported/Configured/Verified/Failed`, never inferred from a setting.
   A requested protection the backend cannot provide blocks connecting
   (fail closed), it is never silently downgraded.
6. **Mock is opt-in and labelled.** Only `AEGIS_BACKEND=mock` selects it; the
   UI shows a persistent banner. If no real backend works, report it as
   unavailable; never fall back to the mock.
7. **Touch only what we own.** Preserve backend profiles missing local metadata. NM connections are ours only if
   `settings::owned_profile` says so (type wireguard + uuid + `aegis-` iface).
   Firewall work must use a dedicated nftables table; never flush global state.
8. **Never claim untested results.** Update `TEST_RESULTS.md` with what you
   actually ran, including failures. A build is not a live test.
9. **Disruptive network tests only in isolation**: the netns peer (split
   route) on a dev host, full-tunnel and kill-switch tests only in the VM
   (`scripts/vm/`). `vm_suite` refuses to run without `/etc/aegis-test-vm`
   and the peer script refuses full modes without `AEGIS_VM_TEST=1`.
10. Tauri app commands are listed in `src-tauri/build.rs` and each needs an
    `allow-*` entry in `capabilities/default.json`. Do not add the shell,
    fs, http or dialog JS permissions.

## Conventions

- Rust 2024, `thiserror` errors, `tracing` logs. User-facing error text says
  how to fix the problem and never includes values from imported files.
- Keep dependencies few; check maintenance and RustSec before adding one.
- Frontend: Tailwind tokens in `src/styles.css` (light + dark), Biome lint,
  native elements first (`<dialog>`, radio inputs) for accessibility.
- Commits: conventional prefixes (`feat:`, `fix:`, `docs:`, `test:`).

## Locations

- A server uses its existing ProfileId; endpoints and credentials have one source
  of truth. Metadata is `ProfileMeta.server` with serde defaults for old stores.
- Country catalog: `crates/aegis-core/data/countries.json` (iso-codes); bundled
  flag assets: `apps/desktop/public/flags/`. Preserve upstream licenses.
- Generated client/preshared keys require remote registration again. Keep the
  pending-registration gate; existing private keys enter only via native import.
- Success history needs actual peer evidence. Never invent RTT/load/geography.
- Preflight a switch before teardown, retain cancellation throughout preflight,
  and refuse protected switching until continuous protection is implemented.
- Usage, platform gaps and provisioning boundaries: `docs/features/LOCATIONS.md`.

See the [release checklist](docs/releases/RELEASE_CHECKLIST.md) for native validation gates.
