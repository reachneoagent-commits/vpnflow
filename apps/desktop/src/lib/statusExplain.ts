import type { Tone } from "../components/ui";

/**
 * One plain sentence under each leak-test row saying why it shows its status
 * and whether that is good, fine or needs action. Keyed by row id, then tone:
 * ok = good, pending = look at this, bad = act now, none = not checked / off.
 */
const EXPLAIN: Record<string, Partial<Record<Tone, string>>> = {
  kill_switch: {
    ok: "Good: if the VPN drops, your internet stops instead of leaking your real address.",
    pending: "Not confirmed yet. Wait a moment, or press Test again.",
    bad: "Needs action: the safety net did not start. Your real address could show if the VPN drops. Disconnect and connect again.",
    none: "Off: if the VPN drops, your traffic can go out on your normal connection. You can turn it on in Settings.",
  },
  dns: {
    ok: "Good: the lookups that turn site names into addresses go through the VPN, so your internet provider cannot see which sites you visit.",
    pending: "Not fully confirmed yet. Press Test again in a moment.",
    bad: "Needs action: some lookups reach your internet provider, who can see which sites you visit. Disconnect and connect again.",
    none: "Not checked yet, or this computer cannot check it.",
  },
  ipv6: {
    ok: "Good: newer-style addresses are covered by the VPN or blocked, so they cannot show your real address.",
    pending: "Not fully confirmed yet. Press Test again in a moment.",
    bad: "Needs action: your real address could show through a newer-style (IPv6) connection. Disconnect and connect again.",
    none: "Not checked yet, or this computer cannot check it.",
  },
  reachability: {
    ok: "Good: the server answered through the tunnel, so the VPN is working. This is not a speed test.",
    pending: "The server answered slowly or only once. Wait a moment, or try another server.",
    bad: "Needs action: the server did not answer. Try another server in the same country.",
    none: "Not checked yet. It runs once the server answers.",
  },
  public_ip: {
    ok: "Good: websites see the VPN server's address, not your home address.",
    pending:
      "We could not confirm which address websites see. This is not a leak by itself; press Test again.",
    bad: "Needs action: websites may still see your real address. Disconnect and connect again.",
    none: "Not checked yet.",
  },
  other_vpn: {
    ok: "Good: no other VPN is running, so two VPNs cannot fight over your connection.",
    pending:
      "Another VPN is running at the same time. Two VPNs can cause drops or leaks. Press Stop it, or quit that app.",
    bad: "Needs action: another VPN is interfering. Quit it, then press Test again.",
    none: "Not checked yet.",
  },
  browser_dns: {
    ok: "Good: no browser we know about uses its own DNS service, so your browser follows the VPN's DNS.",
    pending:
      "A browser sends its lookups to its own service and skips the VPN's DNS (and ad blocking). Your pages still go through the VPN. Turn off 'secure DNS' in that browser if you want the VPN to handle it.",
    bad: "Needs action: turn off the browser's own secure DNS so lookups stay inside the VPN.",
    none: "Nothing to check, and that is fine: none of the browsers we look at (Chrome, Edge, Brave, Arc, Vivaldi, Firefox) has a setting on this computer. Browsers without their own DNS setting follow the computer's DNS, which the VPN covers.",
  },
};

export function statusExplain(rowId: string, tone: Tone): string | undefined {
  return EXPLAIN[rowId]?.[tone];
}
