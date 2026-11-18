#!/bin/bash
# VPNFlow leak and kill-switch check for macOS. Run while connected, with the
# kill switch turned ON in Settings and Cloudflare WARP off:
#   sudo bash scripts/macos/verify-vpn.sh
# It prints PASS / FAIL / WARN per check and saves a report next to this repo.
set -u

if [ "$(id -u)" -ne 0 ]; then
  echo "Run with sudo: sudo bash $0"; exit 2
fi

REPORT_DIR="$(cd "$(dirname "$0")/../.." && pwd)/docs/testing"
mkdir -p "$REPORT_DIR"
REPORT="$REPORT_DIR/verify-report-$(date +%Y%m%d-%H%M%S).txt"
exec > >(tee "$REPORT") 2>&1

ANCHOR="com.apple/vpnflow"
PASS=0; FAIL=0; WARN=0
pass() { echo "  PASS  $*"; PASS=$((PASS+1)); }
fail() { echo "  FAIL  $*"; FAIL=$((FAIL+1)); }
warn() { echo "  WARN  $*"; WARN=$((WARN+1)); }
myip() { curl -4 -s --max-time "${1:-5}" https://api.ipify.org 2>/dev/null; }

echo "VPNFlow verification  $(date)"
echo

echo "[1] Tunnel and public IP"
if pgrep -f wireguard-go >/dev/null; then
  # wireguard-go is started with plain "utun"; the kernel picks the number.
  # The tunnel is the utun that has an IPv4 address (system utuns are IPv6-only).
  for u in $(ifconfig -l | tr ' ' '\n' | grep '^utun'); do
    if ifconfig "$u" | grep -q 'inet [0-9]'; then IFACE=$u; break; fi
  done
fi
if [ -z "${IFACE:-}" ]; then
  fail "no wireguard-go tunnel is running. Connect in VPNFlow first."
  echo; echo "Result: cannot continue."; exit 1
fi
pass "tunnel interface $IFACE is up"
TUNNEL_IP=$(myip)
if [ -z "$TUNNEL_IP" ]; then fail "could not read a public IP"; exit 1; fi
echo "        public IP seen by websites: $TUNNEL_IP"
echo "        $(curl -s --max-time 5 "https://ipinfo.io/$TUNNEL_IP/org" 2>/dev/null)"
pass "public IP recorded (confirm above that it is NOT your home ISP)"

echo
echo "[2] DNS"
SERVERS=$(scutil --dns | awk '/nameserver\[[0-9]+\]/ {print $3}' | sort -u | tr '\n' ' ')
echo "        system DNS servers: $SERVERS"
if [ "$(echo $SERVERS | wc -w | tr -d ' ')" = "1" ]; then
  pass "only one DNS server is configured"
else
  fail "more than one DNS server is configured; others may leak"
fi
DNS_ROUTE=$(route -n get "$(echo $SERVERS | awk '{print $1}')" 2>/dev/null | awk '/interface:/ {print $2}')
if [ "$DNS_ROUTE" = "$IFACE" ]; then
  pass "DNS server is reached through the tunnel ($IFACE)"
else
  fail "DNS server is reached through ${DNS_ROUTE:-unknown}, not $IFACE"
fi
if dscacheutil -q host -a name example.com | grep -q ip_address; then
  pass "name lookups work"
else
  fail "name lookups fail while connected"
fi
R1111=$(route -n get 1.1.1.1 2>/dev/null | awk '/interface:/ {print $2}')
[ "$R1111" = "$IFACE" ] && pass "traffic to the public internet routes via $IFACE" \
  || fail "route to the internet goes via ${R1111:-unknown}, not $IFACE"

echo
echo "[3] IPv6"
V6=$(curl -6 -s --max-time 6 https://api64.ipify.org 2>/dev/null)
if [ -z "$V6" ]; then
  pass "IPv6 traffic does not get out (blocked or unavailable)"
elif echo "$V6" | grep -q ':'; then
  warn "an IPv6 address answered: $V6. Check it is not your ISP's before trusting this."
else
  pass "IPv6 request fell back to IPv4 ($V6)"
fi

echo
echo "[4] Kill switch rules"
RULES=$(pfctl -a "$ANCHOR" -sr 2>&1)
if echo "$RULES" | grep -q "block drop"; then
  pass "kill-switch rules are loaded"
else
  fail "kill-switch rules are not loaded. Turn the kill switch ON in Settings and reconnect."
  echo "$RULES" | sed 's/^/        /'
fi
if pfctl -s info 2>/dev/null | grep -q "Status: Enabled"; then
  pass "pf firewall is enabled"
else
  fail "pf firewall is not enabled; the rules would not be enforced"
fi

echo
echo "[5] Kill switch under attack: killing the tunnel process"
LEAKED=0; BLOCKED=0; RECOVERED=""
pkill wireguard-go
START=$(date +%s)
while [ $(( $(date +%s) - START )) -lt 40 ]; do
  IP=$(myip 2)
  if [ -z "$IP" ]; then
    BLOCKED=$((BLOCKED+1))
  elif [ "$IP" != "$TUNNEL_IP" ]; then
    LEAKED=$((LEAKED+1)); echo "        LEAK: saw $IP at +$(( $(date +%s) - START ))s"
  else
    RECOVERED="+$(( $(date +%s) - START ))s"; break
  fi
done
echo "        attempts blocked while tunnel was down: $BLOCKED"
[ "$LEAKED" -eq 0 ] && pass "no traffic escaped with a different IP" \
  || fail "$LEAKED request(s) escaped the tunnel with a different IP"
if [ "$BLOCKED" -gt 0 ]; then
  pass "traffic was blocked while the tunnel was down"
else
  warn "tunnel came back before any request was blocked; the gap was too short to prove blocking"
fi
if [ -n "$RECOVERED" ]; then
  pass "VPNFlow reconnected on its own ($RECOVERED)"
else
  warn "VPNFlow did not reconnect within 40s (traffic stayed blocked, which is safe). Reconnect in the app."
fi

echo
echo "Summary: $PASS passed, $FAIL failed, $WARN warnings"
echo "Report saved: $REPORT"
[ "$FAIL" -eq 0 ]
