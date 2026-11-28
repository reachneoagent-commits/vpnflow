#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
export AEGIS_TEST_CALLS="$WORK/calls"
cat >"$WORK/helper" <<'EOF'
#!/bin/sh
test "$1" = remove
echo called >>"$AEGIS_TEST_CALLS"
exit "${AEGIS_TEST_FAILURE:-0}"
EOF
chmod +x "$WORK/helper"
for kind in deb rpm; do
	case $kind in
	deb) hook=deb-prerm.sh ;;
	rpm) hook=rpm-preun.sh ;;
	esac
	sed "s|/usr/libexec/aegis-vpn/aegis-helper|$WORK/helper|" "$ROOT/apps/desktop/src-tauri/packaging/$hook" >"$WORK/$kind"
done
sh "$WORK/deb" upgrade
sh "$WORK/rpm" 1
test ! -e "$AEGIS_TEST_CALLS"
sh "$WORK/deb" remove
sh "$WORK/rpm" 0
test "$(wc -l <"$AEGIS_TEST_CALLS")" -eq 2
if AEGIS_TEST_FAILURE=1 sh "$WORK/deb" remove; then exit 1; fi
if AEGIS_TEST_FAILURE=1 sh "$WORK/rpm" 0; then exit 1; fi
echo 'PASS package hooks preserve blocks on upgrade and stop removal on cleanup failure'
