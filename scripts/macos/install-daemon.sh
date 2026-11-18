#!/usr/bin/env bash
# Install (or remove) the VPNFlow privileged daemon on macOS. Development use;
# the signed installer package will ship the same files.
#
#   brew install wireguard-go
#   cargo build --release -p vpnflow-daemon
#   sudo scripts/macos/install-daemon.sh install
#   sudo scripts/macos/install-daemon.sh uninstall
#
# Optional: sudo scripts/macos/install-daemon.sh install <daemon> <wireguard-go>
set -euo pipefail

LIBEXEC=/usr/local/libexec/vpnflow
PLIST=/Library/LaunchDaemons/com.vpnflow.daemon.plist
LABEL=com.vpnflow.daemon
STATE=/var/db/vpnflow
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

die() {
	echo "error: $*" >&2
	exit 1
}

find_wireguard_go() {
	local c
	for c in "${1:-}" /opt/homebrew/bin/wireguard-go /usr/local/bin/wireguard-go; do
		[[ -n $c && -x $c && ! -L $c ]] && {
			echo "$c"
			return
		}
	done
	# Homebrew links into bin; resolve the real file once, never a symlink we install.
	for c in /opt/homebrew/bin/wireguard-go /usr/local/bin/wireguard-go; do
		[[ -L $c ]] && {
			readlink -f "$c" 2>/dev/null || python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$c"
			return
		}
	done
	return 1
}

install_all() {
	local daemon="${1:-$ROOT/target/release/vpnflow-daemon}"
	local wg
	wg="$(find_wireguard_go "${2:-}")" || die "wireguard-go not found; run: brew install wireguard-go"
	[[ -f $daemon ]] || die "daemon not built; run: cargo build --release -p vpnflow-daemon"
	[[ -f $wg ]] || die "wireguard-go is not a regular file: $wg"

	# Stop a running copy first so its files can be replaced.
	launchctl bootout "system/$LABEL" 2>/dev/null || true

	install -d -o root -g wheel -m 0755 /usr/local/libexec "$LIBEXEC"
	install -o root -g wheel -m 0755 "$daemon" "$LIBEXEC/vpnflow-daemon"
	install -o root -g wheel -m 0755 "$wg" "$LIBEXEC/wireguard-go"
	install -d -o root -g wheel -m 0700 "$STATE"
	install -o root -g wheel -m 0644 "$ROOT/packaging/macos/$LABEL.plist" "$PLIST"
	plutil -lint "$PLIST" >/dev/null || die "plist is invalid"

	launchctl bootstrap system "$PLIST"
	sleep 1
	[[ -S /var/run/vpnflow.sock ]] || die "daemon did not start; see /var/log/vpnflow-daemon.log"
	echo "installed. wireguard-go: $wg"
	echo "daemon is listening on /var/run/vpnflow.sock"
}

uninstall_all() {
	launchctl bootout "system/$LABEL" 2>/dev/null || true
	# Remove our firewall anchor so uninstalling never leaves you offline.
	pfctl -a com.apple/vpnflow -F all 2>/dev/null || true
	rm -f "$PLIST" /var/run/vpnflow.sock /var/run/vpnflow-tun.name
	rm -f "$LIBEXEC/vpnflow-daemon" "$LIBEXEC/wireguard-go"
	rmdir "$LIBEXEC" 2>/dev/null || true
	echo "removed the daemon and kill-switch rules."
	echo "profiles are kept in $STATE; delete it to remove stored keys:  sudo rm -rf $STATE"
	if [[ -f $STATE/dns-backup.json ]]; then
		echo "note: $STATE/dns-backup.json exists, so DNS may not have been restored."
		echo "      Reset it in System Settings > Network > (your network) > DNS."
	fi
}

main() {
	[[ $(uname) == Darwin ]] || die "macOS only"
	[[ $EUID -eq 0 ]] || die "run as root (sudo)"
	case "${1:-}" in
	install) shift && install_all "$@" ;;
	uninstall) uninstall_all ;;
	*) die "usage: $0 install [daemon] [wireguard-go] | uninstall" ;;
	esac
}
main "$@"
