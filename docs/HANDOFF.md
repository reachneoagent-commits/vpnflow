# Handoff (as of v0.1.7, 2026-11-19)

Read this first in a new chat. It replaces re-reading history. Project rules are in AGENTS.md, plan in ROADMAP.md.

## User
Non-technical about VPNs and IT, tests as a brand-new user. Use plain language in the UI and in replies. Every new feature ships a "?" help card (`apps/desktop/src/lib/help.ts`) with options explained and a recommendation. Replies: short, result first.

## Shipped this session (all on origin/main, v0.1.7 published, 4 builds)
- 0.1.4: bulk delete in Advanced WireGuard list and Locations server list.
- 0.1.4+: one bad file no longer aborts a multi-file import.
- 0.1.5: import auto-fix, IPv4-only addresses with a `::/0` route import as IPv4-only.
- 0.1.6: leak test measures live, shows a "Checking" state.
- 0.1.7: "Import folder" (folder plus one level of sub-folders, `.conf` only), Locations and Advanced WireGuard merged into one page (Advanced is a collapsible section).

## NOT verified in the real app
Folder import, merged page, live leak test, bulk delete, IPv4-only auto-fix. User is testing 0.1.7 now. Wait for their feedback before assuming they work.

## Known gaps / open items
- Engine ignores the `fixed` flag from `parse_import`; no extra note tells the user a server was imported IPv4-only beyond the existing profile warning.
- Kill switch: Linux behaviour read from code (block kept through drops, removed only by Disconnect or Cancel, gone on reboot). Mac and Windows backends not checked for crash and quit behaviour. Settings cannot change mid-session.
- Linux helper tests fail in this sandbox (`services/linux-helper`).
- Windows service is a skeleton. Developer ID signing not done (Mac builds unsigned).
- Old `brand/` files remain in git history (removal needs the user's say-so).
- GitHub repo still marked "Forked from mikeholp87/aegis-vpn" (detaching is permanent, via GitHub support; left alone).
- `Config files/` holds private Proton keys: gitignored (`*.conf`), never commit. New Proton configs there are not yet imported. `Config files/PROTON-DOWNLOAD-LIST.md` lists servers to download.
- Roadmap items marked "built, awaiting live test" in ROADMAP.md still need the user's live check.

## Release recipe
Bump version in `Cargo.toml`, `apps/desktop/package.json`, `apps/desktop/src-tauri/tauri.conf.json`, then `cargo update --workspace --offline` for `Cargo.lock`. Commit with explicit paths (folder has `core.fileMode=false`). Push, then `gh workflow run release-desktop.yml -f confirm_version=<ver>`. If a macos-15 job fails or is cancelled, `gh run rerun <id> --failed`. Commit trailer: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Checks before a commit
`cargo fmt --all`, clippy, `tsc --noEmit`, Biome (one known CSS specificity warning), Vitest (36 tests). New Tauri commands need entries in `build.rs`, `capabilities/default.json` and a generated permissions toml.

## Tooling gotchas
macOS `sed -i` needs a backup arg; use python for edits. No desktop screenshots for UI checks (see CLAUDE.md).
