# macOS

Status: backend and installer written, **never run end to end on a Mac** (see
`docs/testing/TEST_RESULTS.md`).

## Layout

| Piece | Path |
|---|---|
| App | `/Applications/VPNFlow.app` |
| Root daemon (launchd) | `/usr/local/libexec/vpnflow/vpnflow-daemon run`, socket `/var/run/vpnflow.sock` |
| `wireguard-go` (root-owned, required by the daemon) | `/usr/local/libexec/vpnflow/wireguard-go` |
| State (mode 0700) | `/var/db/vpnflow` |
| Kill switch | pf anchor `com.apple/vpnflow` |
| launchd job | `/Library/LaunchDaemons/com.vpnflow.daemon.plist` |

## Installer

```
scripts/macos/build-pkg.sh                 # dist/VPNFlow-<version>.pkg, host arch
ARCHS="arm64 x86_64" scripts/macos/build-pkg.sh
SIGN_APP="Developer ID Application: …" SIGN_PKG="Developer ID Installer: …" scripts/macos/build-pkg.sh
```

It builds `wireguard-go` from the pinned tag (`WG_GO_TAG`; set `WG_GO_COMMIT` to
pin the exact commit), the daemon and the app, then packs them with
`packaging/macos/scripts/{preinstall,postinstall}`. Unsigned packages open with
right-click > Open. Notarization needs an Apple Developer ID (not done).
`pkgutil --payload-files` shows `._` entries; these are xattrs from
`com.apple.provenance` and the installer merges them back into attributes.

Development without a package: `sudo scripts/macos/install-daemon.sh install|uninstall`.

## Uninstall

`sudo scripts/macos/install-daemon.sh uninstall` stops the daemon and flushes the pf
anchor. Remove the app from `/Applications`; delete `/var/db/vpnflow` to drop stored keys.
