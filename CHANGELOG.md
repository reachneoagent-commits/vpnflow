# Changelog

## Unreleased

- "Skip the VPN for these sites" checks the name as you type: a red message
  under the box says what to fix (missing .com ending, spaces, an email or a
  number address, already listed), and a valid entry shows the name it will
  be saved as. www. and https:// stay optional. A wrong entry is no longer
  cleared from the box.
- New installs start with friendlier defaults: Allow local network and Let
  Tailscale through are on, and setup turns on Start VPNFlow when you sign in
  and Connect on launch. Existing settings are not changed.
- Connect on launch works without a preferred profile: it uses Quick connect.
- Mac leak test: a DNS resolver for one domain whose servers are all tailnet
  addresses (VPNFlow's `/etc/resolver/<tailnet>` pointing at MagicDNS) no
  longer counts as a DNS leak. Tailscale's "Use Tailscale DNS" (MagicDNS for
  every name) is still reported.
- New Settings list "Skip the VPN for these sites" (Mac), empty by default,
  for sites that block VPN users. While connected, the app looks each site
  and its `www.` name up every 20 seconds (through the VPN's DNS) and the
  daemon routes the addresses around the tunnel; the kill switch passes them
  on ports 80/443 only. Those sites see your real IP; the "?" card says so.
- Kill switch (Mac and Linux) lets Tailscale/TailFlow addresses through, so a
  mesh VPN keeps working beside VPNFlow; the rest of the internet stays blocked.
  Tailscale's own direct connections (UDP port 41641) are let through too.
- New Settings switch "Let Tailscale through" (Mac): while the kill switch
  blocks, the Tailscale app can still reach Tailscale's relay (DERP) and
  sign-in servers on TCP 80/443 and UDP 3478 only. The relay list is fetched
  from Tailscale, cached, refreshed every 6 hours and applied live.
- With "Let Tailscale through" on, connecting no longer breaks all DNS when
  Tailscale's "Use Tailscale DNS" is on. While connected, VPNFlow turns that
  option off with Tailscale's CLI and the daemon writes a marked
  `/etc/resolver/<tailnet domain>`, so only tailnet names go to Tailscale.
  Both are undone on disconnect; the daemon removes a stale file at start.
- Mac: while connected, the daemon adds a default route scoped to the Wi-Fi
  or Ethernet interface (and removes it on disconnect). Without it, apps
  that bind to that interface themselves, like Tailscale, got "network
  unreachable" and could not reach their servers. The kill switch still
  filters that traffic.
- The relay list is resent every minute, so a restarted daemon gets it back
  quickly.
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
