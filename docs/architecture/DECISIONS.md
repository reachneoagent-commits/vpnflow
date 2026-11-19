# Decisions

Each entry: decision, why, alternatives, revisit when.

### D1. Linux tunnels through NetworkManager, no root helper (2026-11-16)
NM ≥1.16 has native WireGuard connections and Polkit already authorizes the
active desktop user. The app stays unprivileged; NM does routes/DNS and
cleans up its own interface. *Alternatives:* own root helper with netlink +
nftables (more attack surface, duplicate of NM). *Revisit:* systems without NM
need a helper-based fallback (planned).

### D2. Secrets live with the privileged backend
Linux: NM system-owned secret (root-only keyfile, survives GUI exit, same as
Wi-Fi passwords). Windows: service + DPAPI. The app's own files hold only
metadata. *Alternative:* Secret Service/libsecret agent-owned secrets. Rejected
for v1: tunnel and reconnect would depend on the GUI running and on a
keyring being unlocked. *Trade-off:* root on the machine can read keys
(true of any VPN client).

### D3. Webview never handles keys or file paths
Editor offers Keep/Generate/Remove for keys; pasting keys is replaced by
importing a `.conf`. Dialogs run in Rust. Limits damage from compromised
web content to "use the commands the UI already has".

### D4. Explicit per-command Tauri permissions
`build.rs` declares the command list, so each command needs an `allow-*`
entry. No shell/fs/http/dialog JS plugins. Strict CSP, `freezePrototype`.

### D5. "Connected" is not "confirmed"
Unprivileged code cannot read WireGuard handshake times on Linux. We treat
authenticated data received (`rx_bytes > 0`) as proof the server answered,
show `Pending` until then and `NoResponse` after 20 s of unanswered traffic.

### D6. Fail closed on unavailable protection
If the kill switch is on in Settings but unsupported, `connect` refuses with a
fix-it message instead of connecting unprotected.

### D7. zbus 6 for the NetworkManager backend
First shipped on zbus 5.19; migrated to 6.3 on 2026-11-16 after RustSec
flagged zbus 5's zvariant, zvariant_derive and zbus_names as unmaintained
(RUSTSEC-2026-0370/71/72; zbus 6 merged them in). Our code uses
`zbus::{Variant, OwnedVariant, OwnedObjectPath, Proxy}` with features
`tokio` + `proxy`. tauri-plugin-notification was replaced by `notify-rust`
4.19 (`z-with-tokio`, zbus 6) called from Rust. **Remaining:** zbus 5 is still
pulled in by tauri-plugin-single-instance 2.5.5 (latest release), which we keep
because re-launching is how users get the window back on desktops without a
tray. *Revisit:* bump it when it moves to zbus 6.

### D8. Fewer crates than the original outline
Storage and diagnostics live in `aegis-core`; IPC crate deferred until the
helper/service exist. Split again when a module grows or gets a second user.

### D9. Bundled UI font (revised 2026-11-16)
Atkinson Hyperlegible Next and Mono (OFL-1.1, via Fontsource) are bundled into
the app, chosen because they disambiguate 0/O and 1/l/I in keys, addresses and
endpoints. Still zero network requests; CSP `font-src 'self'` unchanged.
Previously: system UI font only.

### D10. Mock only on explicit request
`AEGIS_BACKEND=mock`; never a silent fallback. Banner always visible.

### D11. Leaked NetworkManager anti-spoofing rules: detect and warn, never remove (2026-11-16)
Found in the VM suite: NetworkManager 1.52.1 (Debian 13) adds a raw-table rule
per full tunnel (`ip daddr <tunnel addr> iifname != "<iface>" … drop`). If NM
restarts while the tunnel is up, it re-adds the rule and on disconnect removes
only one copy; the leftover silently drops replies for any later tunnel with
the same address on another interface, while the app still sees encrypted
data arriving ("server responded"). Reproduced with a plain
`nmcli connection import` profile, so it is not Aegis-specific.
*Decision (user, 2026-11-16):* Aegis will **detect** such leftovers and show a
warning (reboot, or reload the firewall), and will **not remove** them, because
they belong to NetworkManager. Reading the ruleset needs CAP_NET_ADMIN, so the
check ships with the root helper (Phase E); there is no unprivileged
equivalent. *Also:* report upstream after checking current NM releases.

### D12. DNS and IPv6 protection: block, then verify at runtime (2026-11-16)
*IPv6:* an IPv4 full tunnel that does not carry IPv6 gets an NM-owned
`::/0` route on the tunnel device (metric 1, `ipv6.method=manual` without
addresses; tested on NM 1.52) instead of leaking. WireGuard has no IPv6 peer,
so it drops that traffic. No firewall or root is needed. A `type=unreachable`
route was tried first and rejected: it lives on `lo`, and after an NM restart
while connected NM left it behind on disconnect (IPv6 stayed blocked). A
device route is removed by the kernel with the interface. Connections
installed earlier are refreshed on activation (`settings::lacks_ipv6_block`).
*Verification:* while connected, every status poll runs unprivileged checks
(`aegis-linux/src/verify.rs`): a UDP `connect()` route lookup (no packets
sent) must pick a tunnel source address, fail with no route, or the
lowest-metric default route in `/proc/net/ipv6_route` must be the tunnel
device (needed because without a tunnel IPv6 address the kernel picks the
uplink's source address). Assumes no foreign IPv6 policy rules. DNS servers
must route through the tunnel, and systemd-resolved's `Domains` must give the
`~.` catch-all to the tunnel link only (or `/etc/resolv.conf` must list only
tunnel servers). Pass ⇒ `Verified`, otherwise `Failed`. This checks routing
and resolver state, not packets on the wire; the VM suite does that.
*Not covered:* traffic during reconnects or tunnel failure (needs the kill
switch), IPv4 on IPv6-only full tunnels, local-network names (for example
`*.lan`) that resolved deliberately sends to that network's DNS.


### D13. Internet reachability: one DNS lookup through the tunnel (user, 2026-11-16)
While connected, Aegis asks the profile's own DNS server, reached through the
tunnel, to resolve `example.com` (RFC 2606) when the tunnel comes up and
then once a minute. NOERROR/NXDOMAIN ⇒ `Verified` ("tunnel DNS answered"),
other DNS errors or no reply within 3 s ⇒ `Failed`. It is skipped
(`Unsupported`) if no profile DNS server routes through the tunnel, so the
check never sends anything outside it and contacts no new party. *Limit:* a
resolver cache may answer, so it proves the far end's resolver works, not
that arbitrary internet hosts are reachable. Chosen over an opt-in TCP check
to a user host, which would contact a third party.

### D15. Locations are metadata on existing profiles (2026-11-16)
One ProfileId represents one server entry, including multi-peer configurations.
`ProfileMeta.server` stores additive serde-default location/preferences/history;
endpoints, AllowedIPs, credentials and connection lifecycle remain in their
existing owners. Alternatives: separate registry crate/database and independent
server IDs would duplicate joins and recovery state without another consumer.
Countries use a separately bundled maintained ISO catalog; no endpoint geolocation
is inferred. Existing profiles stay unassigned until the user adds metadata.

Quick Connect ranks recent authenticated observations and preferences, not
fabricated latency or throughput. No RTT probing or idle tunnel activation is
introduced. Generated identities remain pending remote registration. Sequential
switches use the same operation lock and validate before teardown; protected
switches are refused because the current backends provide no continuous kill
switch. A failed destination stays explicitly failed after cleanup rather than
automatically activating another profile. Revisit Windows and protected switching
only with a tested privileged service/helper and controlled live-platform tests.

### D14. Kill switch: pkexec one-shot root helper, session-scoped (2026-11-16; revised by user 2026-11-17)
`services/linux-helper` (`aegis-helper`, with libc for no-follow opens and
kernel file locking) is the only code that runs as root. The app starts it
through pkexec. Firewall changes affect **all local users**, so the user chose
administrator authentication for both `org.aegisvpn.helper.killswitch.apply`
and `.remove`. Read-only `.status` and `.handshake` actions remain password-free
for the active desktop user; every action is scoped to its first argument.
It manages one nftables table,
`inet aegis_vpn` (output and input chains, policy drop, priority filter-10),
in a single atomic transaction and reads it back before reporting success.
Startup inspects the kernel table through `status`; marker files are no longer
written or treated as proof. A persistent root-owned lock in `/run/aegis-vpn`
serializes apply/remove; its directory and descriptor are checked before use.
Inspection failures remain unknown and Disconnect can retry cleanup; readback
failure preserves the block. Uninstall retains the helper if cleanup fails.
Arguments are strictly validated (`aegis-<8 hex>` interface,
1–16 `IP:port` endpoints); `nft` runs by absolute path with a clean env.
Allowed while blocking: loopback, the tunnel interface, UDP to the endpoints,
DHCPv4/v6, IPv6 neighbour discovery; with *Allow local network* also
private/link-local/multicast ranges. Outbound has no `established`
exception, so connections opened before the block cannot continue outside
the tunnel after a drop.
*Lifetime:* applied before activation (nothing can slip out while the
tunnel comes up) and kept through drops, reconnects, failed attempts, NM
restarts and app crashes; removed only by Disconnect/Cancel ("Stop blocking"
in the UI). Settings cannot change mid-session.
*Limits:* endpoints must be IP literals (a name cannot be resolved while
blocked; such profiles are refused with a fix-it message). Forwarded traffic
(containers, VMs) is not covered: no forward chain. Rules vanish on reboot
so the block does not persist across reboots. Protected
profile switching stays refused. *Alternatives:* systemd root service
(rejected: permanent root process and socket). *Revisit:* forward chain,
hostname endpoints, Windows (WFP).

### D15. Sleep/resume, network switching and a silent server (2026-11-17)
*Events.* `aegis-linux/src/events.rs` watches the system D-Bus for logind's
`PrepareForSleep(false)` (⇒ `SystemEvent::Resumed`) and for NetworkManager's
`PrimaryConnection` property changing (⇒ `NetworkChanged`: other Wi-Fi, cable
plugged or removed, different uplink). The engine collapses a burst (a real
switch emitted three at once) into one reaction after 3 s, ignores network
events for 15 s after its own activation, and only reacts while a session is up.
The property stream also delivers the current value on subscribe; that first
event is harmless for the same reason.
*Reaction.* The tunnel is not torn down: WireGuard roams and re-handshakes on
its own, and the kill switch is not touched. Proof of a response from *before*
the event stops counting: peer goes back to `Pending` until a handshake newer
than the event or newly received bytes arrive; reachability, public IP and
protection checks are cleared and re-run. Bringing the tunnel up with a stale
"Confirmed" would be a lie after a network change.
*Resume.* The engine remembers the profile of a session that came up
(`session`, cleared by Disconnect/Cancel). If it ended as `Failed` (retries ran
out while asleep) and automatic reconnection is on, a resume starts one fresh
connect, which re-applies the kill-switch block. A session the user ended, or
one that never came up, is never revived. With auto-reconnect off nothing is
restarted.
*Silent server.* Independent of OS events: data sent for 30 s with nothing
received flips a confirmed peer to `NoResponse` ("The server stopped
answering…"); it returns to `Confirmed` when anything arrives. WireGuard
answers received data with a keepalive within ~10 s, so this does not fire on
an idle tunnel. The tunnel and the block are kept.
*Not verified.* A real suspend (a VM cannot suspend like hardware), a real
Wi-Fi roam, and whether NetworkManager re-adds the endpoint route after a real
uplink change: if it does not, the dashboard shows `NoResponse` but Aegis does
not yet re-activate the tunnel by itself. *Revisit:* re-activate (keeping the
block) when `NoResponse` follows a network change.

## D16: Windows backend design (2026-11-17; pipe layer run on Windows, the rest not)

*Shape.* The app (unprivileged) holds `ServiceBackend`, a client of a LocalSystem
service over the named pipe `\\.\pipe\aegis-vpn-service`: one 4-byte length +
JSON frame per request/reply (`aegis-windows::proto`, 1 MiB-class bound checked
before allocation, `SecretText` has a redacted `Debug`). The service is the only
privileged code on Windows, like `aegis-helper` on Linux.
*Tunnels.* Official WireGuard embeddable-dll-service, per its README: the service
installs `WireGuardTunnel$<name>` (own-process, auto start, depends on `Nsi` and
`TcpIp`, `SERVICE_SID_TYPE_UNRESTRICTED`) running our exe as `<exe> /service
<conf>`, which loads `tunnel.dll` by absolute path and calls the exported
`WireGuardTunnelService(LPCWSTR)`. `<name>` is `aegis-` + 24 hex of the profile
id (32-char WireGuard limit; service names are case-insensitive); only names of
exactly that shape are ever touched (`names`, unit-tested).
*Tunnel host (implemented, loads tested, not run under the SCM).* Each tunnel
service runs `aegis-tunnel.exe /service <…\aegis-<24 hex>.conf.dpapi>`
(`services/windows-service`, `tunnel_host` + `tunnel_dll`). It restricts the process
DLL search (`SetDefaultDllDirectories`), validates that the argument is exactly one
of our encrypted configs in our directory, and loads `tunnel.dll` from its own
directory by absolute path with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
LOAD_LIBRARY_SEARCH_SYSTEM32`. **Never `FreeLibrary` it:** a Go c-shared DLL
crashed the process with `STATUS_ACCESS_VIOLATION` when unloaded (measured).
*Secrets (read from wireguard-windows `6ece77b`, 2026-09-22; format verified against its real loader as a normal user, not yet as SYSTEM).*
`WireGuardTunnelService(conf)` calls `conf.LoadFromPath`, which accepts either a
plaintext `<name>.conf` or **`<name>.conf.dpapi`**. For `.dpapi` it calls
`CryptUnprotectData` (flags `CRYPTPROTECT_UI_FORBIDDEN` only: user scope of the
calling account, no extra entropy) and requires the DPAPI description to equal
the tunnel name. The tunnel service and our service are both LocalSystem, so the
service stores `ProgramData\VPNFlow\<name>.conf.dpapi` encrypted with
`CryptProtectData(description = name)` and the tunnel reads it directly: **no
plaintext config ever touches disk**, which replaces the earlier plaintext plan.
Implemented in `services/windows-service/src/{dpapi,store}.rs`. The full profile
id (a tunnel name holds only 24 of its 32 hex digits) is kept as a leading
`# aegis-profile-id = …` comment inside the encrypted payload, which
WireGuard's parser ignores. The directory is created with a protected SYSTEM +
Administrators DACL, and an existing one is accepted only if its owner is SYSTEM
or Administrators and its DACL matches exactly, because `ProgramData` lets any
user pre-create a folder and then swap a config. The service re-parses and
re-validates every `Install` itself and stores the validated canonical form. `aegis-<24 hex>` passes `TunnelNameIsValid`
(`^[a-zA-Z0-9_=+.-]{1,32}$`, no `$`). Verified: a file written by our `ConfigStore`
is loaded by upstream's `conf.LoadFromPath` (positive and two negative controls,
TEST_RESULTS). Still to verify: encrypt as SYSTEM and start the real tunnel service.
*The official tunnel firewall is not our kill switch (source read, not tested).*
`enableFirewall` runs only when the config has exactly one peer, no `Table = off`
and an AllowedIPs entry that is a masked `/0`. It then installs, in a **dynamic**
WFP session, permit rules for the tunnel service process (unrestricted), loopback,
the tunnel interface, DHCP and NDP, DNS blocks except to the tunnel's DNS, and a
block-all at the lowest weight. So: LAN is blocked with no option to allow it;
the filters exist only while the tunnel service process lives, so a crash or stop
leaves traffic **unblocked**, and nothing covers the gap before connect or during
reconnect. It does not meet the `apply_kill_switch` contract (survives a crash,
`allow_lan`, removed on disconnect, our own provider key). Aegis needs its own WFP
filters; they must not conflict with these (the tunnel service is permitted
unrestricted, so the endpoint UDP needs no extra rule from us).
*Pipe security (implemented, tested unelevated on Windows 11).* The pipe has an
explicit protected DACL (`services/windows-service/src/security.rs`): deny network
logons; SYSTEM and Administrators full; interactive users `0x12019b`
(read/write minus `FILE_CREATE_PIPE_INSTANCE`). The first instance uses
`FILE_FLAG_FIRST_PIPE_INSTANCE` and remote clients are rejected. **Measured:**
Windows checks `FILE_CREATE_PIPE_INSTANCE` when a process creates a further
instance with `CreateNamedPipe` (a user holding only the IU ACE gets
`ACCESS_DENIED`), but does *not* check it when a client opens the pipe (every
access mask opened, including `FILE_APPEND_DATA` alone). So users can talk to the
service but cannot stand up a fake instance of its name. The client still opens
with only read + `FILE_WRITE_DATA` at identification impersonation level (least
privilege). The pipe is bounded: 8 concurrent connections, 30 s idle timeout,
frames capped before allocation, mutating requests serialized by one lock, logs
carry command names only. **Not done:** the per-connection check of the client's
token/session; today any interactive local user may use the pipe. Running as
SYSTEM (the production case) is untested; unelevated tests grant the owner an
extra ACE so the server can create follow-up instances.
*Kill switch (planned).* WFP filters under an Aegis-owned provider and sublayer,
persistent so they can outlive the app and be removed by provider key. No
firewall `netsh` shelling.
*Not done.* The service, DPAPI, SCM install, WFP, verification of DNS/IPv6
protection, sleep/resume and network change on Windows. `protection()` reports
`Off` until the service can verify, so nothing is claimed.
