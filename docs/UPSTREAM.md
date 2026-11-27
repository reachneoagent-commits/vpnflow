# Keeping up with upstream

VPNFlow is a fork of [Aegis VPN](https://github.com/mikeholp87/aegis-vpn) (MIT or Apache-2.0).
This is how we take their updates without losing our name, icons, links or features.

## Two kinds of change

1. **Branding** in upstream's files is made **only by `rebrand/apply.py`**, from
   `rebrand/brand.json`. Never hand-edit it: add an entry and run the script.
   - `edits`: product name, `mainBinaryName`, app id `org.vpnflow.desktop`, window title, tray,
     notifications, About, Settings and Locations wording, the source link.
   - `prepend` / `append`: the agent note in `CLAUDE.md`, merge rules in `.gitattributes`.
   - Icons: `rebrand/app-icon/` is copied over `apps/desktop/src-tauri/icons/` (`icons_dir`).
   `apply.py` is the same engine as in the other FlowApps forks.
2. **Features** are ordinary commits that git carries through a merge: the redesigned UI
   (Dashboard, Locations, Profiles, Settings, About, onboarding, help cards), the macOS daemon and
   helper, Android, trusted networks, auto-reconnect and failover, the update check and the Tauri
   updater.

So **do not blanket "take theirs"** on a conflicted file unless it carries branding only.

Left alone on purpose: crate names (`aegis-*`), the Linux helper and its Polkit ids
(`org.aegisvpn.helper.*`), the Windows service name (`AegisVpnService`) and its
`%ProgramData%\Aegis VPN` folder, and Aegis wording in helper, service and test messages. Renaming
them would break installed helpers and touch hundreds of lines on every merge.

## What lives where

| Ours (keep ours in a conflict) | Branding only (take theirs, then run `apply.py`) | Branding and features (resolve by hand, then run `apply.py`) |
|---|---|---|
| `rebrand/`, `brand/`, `docs/UPSTREAM.md`, `README.md`, `NOTICE`, `.github/workflows/` | `apps/desktop/index.html`, `AGENTS.md`, `SECURITY.md` | `apps/desktop/src-tauri/`: `tauri.conf.json`, `Cargo.toml`, `build.rs`, `capabilities/default.json`, `src/*.rs` |
| `apps/desktop/src-tauri/icons/` | everything else from upstream | `apps/desktop/src/`: `App.tsx`, `components/`, `lib/`, `screens/*.tsx`, `styles.css`, tests |
| `crates/aegis-macos/`, `crates/aegis-android/`, `services/macos-daemon/`, `scripts/macos/`, `packaging/macos/` (upstream has none) | | `crates/aegis-config/`, `crates/aegis-core/`, `crates/aegis-windows/src/proto.rs`, `services/windows-service/src/`, `Cargo.toml`, `.gitignore` |

`About.tsx` was rewritten: upstream's `About Aegis VPN` heading has no place in ours, so `apply.py`
has no edit for it. If a merge brings it back, delete it by hand.

## Merging upstream

Set the clone up once, so upstream's tags arrive as `upstream/v…` and never collide with ours:

```bash
git remote add upstream https://github.com/mikeholp87/aegis-vpn.git
git config remote.upstream.tagOpt --no-tags
git config --add remote.upstream.fetch '+refs/tags/*:refs/tags/upstream/*'
git config merge.ours.driver true
```

Never push the `upstream/*` tags. Then:

```bash
git fetch upstream
git checkout -b upstream-sync
git merge upstream/main
```

Resolve conflicts by the table above, then re-apply the branding and check it:

```bash
python3 rebrand/apply.py
python3 rebrand/apply.py --check     # prints nothing and exits 0 when everything is rebranded
git grep -n 'Aegis VPN\|mikeholp87' -- apps   # read what is new
```

`--check` only knows the lines it was taught. Upstream's new user-facing "Aegis VPN" text shows up
in the `git grep`: add an edit for each to `rebrand/brand.json`. If the script stops with
`could not find … to edit`, upstream changed that line: find where the text went and update the
entry.

Build and test (`cargo test --workspace`, `pnpm -C apps/desktop test`), run the app once and look at
it, commit and open a pull request.

`rebrand/apply.py` was checked on 2026-11-26 against upstream `52a81a6` (the merge-base): taking
upstream's version of every changed file except `About.tsx` and re-running the script left no
upstream branding.
