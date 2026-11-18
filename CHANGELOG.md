# Changelog

## Unreleased

- Guard Windows pipe startup until service authentication exists.
- Recheck Linux firewall protection and stop the tunnel on detected loss.
- Bind connected-tunnel DNS probes to WireGuard and check resolver link servers.
- Preserve untracked backend credentials and restore the previous config after
  a failed metadata write.
- Synchronize tray and Settings, and isolate temporary simulation data.
- Add a manual Linux preview packaging workflow and release evidence checklist.
- Locations directory with ISO country metadata and bundled licensed flags
- Persistent server/country favorites, recent peer-confirmed history and Quick Connect
- Import/manual profile workflows with country assignment and key-registration gating
- Validated, cancellable server switching through the existing Linux engine
- Dashboard location synchronization, honest unknown health and explicit failure states
- Isolated VM switching checks and manually triggered Linux networking CI (not run here)
- Windows tunnel service, RTT sampling and protected switching remain unavailable
- WireGuard config parser/validator with script and unknown-key rejection
- Connection engine with explicit states, cancel, reconnect, crash recovery
- Desktop app: dashboard, profiles, diagnostics, settings, about, tray
- Linux NetworkManager backend (live test pending)
- Windows backend stub (reports not implemented)
