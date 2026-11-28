#!/bin/sh
set -eu
# dpkg runs prerm on upgrade too. Keep the existing block across upgrades.
case "${1:-}" in
remove | deconfigure)
	/usr/libexec/aegis-vpn/aegis-helper remove || {
		echo 'Aegis firewall cleanup failed. Package removal stopped. Retry removal after recovery.' >&2
		exit 1
	}
	;;
esac
