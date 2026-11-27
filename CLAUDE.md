<!-- VPNFlow (rebrand/apply.py) -->
# VPNFlow

This repository is **VPNFlow**, a fork of Aegis VPN. Before merging upstream or changing branding, read `docs/UPSTREAM.md`. In short:

- Branding (names, ids, links, icons) in files that came from upstream is made only by `rebrand/apply.py` from `rebrand/brand.json`: add an entry there and run the script, never hand-edit it.
- Features are ordinary commits; resolve their merge conflicts by hand, never with a blanket "take theirs".
- Crate, service and Polkit names stay `aegis*` / `org.aegisvpn.*` on purpose.

# Claude Code instructions

All project rules, layout and commands live in AGENTS.md (shared with other
coding agents). Keep that file the single source of truth.

@AGENTS.md

## Claude-specific notes

- Start a session by reading `docs/HANDOFF.md` (current state, open items,
  release recipe), then `docs/ROADMAP.md`. Read the latest entries in
  `docs/testing/TEST_RESULTS.md` only when testing is the topic.
- Root is required for the live netns test peer; ask the user to run
  `! sudo scripts/test-netns-peer.sh up` rather than running sudo yourself.
- Do not take desktop screenshots to check the UI; use the Vitest suite or ask
  the user to look.
