#!/bin/sh
set -eu
# RPM supplies 0 for final removal and a positive count during upgrade.
if [ "${1:-}" = 0 ]; then
	/usr/libexec/aegis-vpn/aegis-helper remove || {
		echo 'Aegis firewall cleanup failed. Package removal stopped. Retry removal after recovery.' >&2
		exit 1
	}
fi
