#!/usr/bin/env bash
# Copy the macOS helper binaries into the app bundle's staging folder so that
# `tauri build` embeds them and the app can install them itself.
#
#   scripts/macos/stage-helper.sh <vpnflow-daemon> <wireguard-go>
#
# Defaults: target/release/vpnflow-daemon and a Homebrew wireguard-go.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$ROOT/apps/desktop/src-tauri/helper"
daemon="${1:-$ROOT/target/release/vpnflow-daemon}"
wg="${2:-}"
if [[ -z $wg ]]; then
	for c in /opt/homebrew/bin/wireguard-go /usr/local/bin/wireguard-go; do
		[[ -e $c ]] && wg="$(python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$c")" && break
	done
fi
[[ -f $daemon ]] || { echo "error: daemon not found: $daemon (cargo build --release -p vpnflow-daemon)" >&2; exit 1; }
[[ -n $wg && -f $wg ]] || { echo "error: wireguard-go not found (brew install wireguard-go, or pass its path)" >&2; exit 1; }
install -m 0755 "$daemon" "$DEST/vpnflow-daemon"
install -m 0755 "$wg" "$DEST/wireguard-go"
echo "staged:"
shasum -a 256 "$DEST/vpnflow-daemon" "$DEST/wireguard-go"
