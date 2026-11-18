# Security policy

## Reporting a vulnerability

Please report privately through GitHub's **"Report a vulnerability"** button
(Security → Advisories) on this repository. Do not open a public issue.

Include the affected version or commit, platform, steps to reproduce, and impact.
We aim to acknowledge reports within 7 days and to agree on a disclosure date
with you. Credit is given unless you prefer otherwise.

## Scope

In scope: key exposure, privilege escalation through the app or its helpers,
traffic or DNS leaks the app claims to prevent, unsafe handling of imported
profiles, IPC/permission bypasses, installer/updater issues.

Out of scope: vulnerabilities in WireGuard itself, NetworkManager, or the OS
(report those upstream), and attacks that require root on the machine.

## Status

Aegis VPN is pre-alpha and has not been independently audited. See
`docs/security/THREAT_MODEL.md` for what is and is not mitigated.

## Locations and server switching

Country assignments are user metadata, not geographic verification. The registry
contains no private or preshared keys; diagnostic bundles may include endpoints,
location labels and connection timestamps, so review them before sharing.
No server directory probes or geolocation requests run automatically. Read the
Settings disclosure for connected-tunnel connectivity checks and their defaults.
DNS-based resolver egress observations do not verify the client's public VPN IP.

Linux kill-switch support depends on the installed helper and Polkit policy.
Protected switching is still refused; without protection, switching is
disconnect-then-connect. Requested unsupported fail-closed protection blocks
connection rather than silently falling back. Destination validation precedes
teardown, and failed cleanup produces an explicit Uncertain state. Windows
service support and full live cross-platform acceptance are pending; see
`docs/features/LOCATIONS.md` and the test results.

See the [gap audit](docs/audits/FULL_GAP_AUDIT.md) and [recovery guide](docs/security/RECOVERY.md) for current enforcement limits.
