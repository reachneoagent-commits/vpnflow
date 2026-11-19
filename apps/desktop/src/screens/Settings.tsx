import { useId, useState } from "react";
import { Button, InfoTip, Panel, Switch } from "../components/ui";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import type { Settings, Support, Theme } from "../lib/types";

const DNS_PRESETS: { id: string; label: string; note: string; servers: string[] }[] = [
  { id: "vpn", label: "The VPN's own (default)", note: "Private, no blocking.", servers: [] },
  {
    id: "adguard",
    label: "AdGuard: blocks ads and trackers",
    note: "Blocks most ads and trackers on every site and app.",
    servers: ["94.140.14.14", "94.140.15.15"],
  },
  {
    id: "cf-malware",
    label: "Cloudflare: blocks malware",
    note: "Blocks known malware sites. Does not block ads.",
    servers: ["1.1.1.2", "1.0.0.2"],
  },
  {
    id: "quad9",
    label: "Quad9: blocks dangerous sites",
    note: "Blocks known phishing and malware sites. Does not block ads.",
    servers: ["9.9.9.9", "149.112.112.112"],
  },
  { id: "cf", label: "Cloudflare: fast, no blocking", note: "No blocking.", servers: ["1.1.1.1", "1.0.0.1"] },
];

function dnsPresetFor(servers: string[]) {
  if (servers.length === 0) return "vpn";
  return DNS_PRESETS.find((p) => p.servers.join() === servers.join())?.id ?? "custom";
}

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function supportNote(s: Support | undefined, what: string) {
  if (!s || s.status === "supported") return undefined;
  return `${what} is not available on this system: ${s.detail}`;
}

export function SettingsScreen() {
  const { settings, saveSettings, caps, profiles, info } = useStore();
  const themeId = useId();
  const prefId = useId();
  const attemptsId = useId();
  const bypassId = useId();
  const dnsId = useId();
  const dnsCustomId = useId();
  const [dnsCustom, setDnsCustom] = useState(false);
  const [dnsDraft, setDnsDraft] = useState("");
  const [bypassDraft, setBypassDraft] = useState("");
  const [trustDraft, setTrustDraft] = useState("");
  const [trustMsg, setTrustMsg] = useState<string | null>(null);
  if (!settings) return null;
  const set = (patch: Partial<Settings>) => saveSettings({ ...settings, ...patch });
  const ksUnsupported = caps?.kill_switch.status !== "supported";

  return (
    <div className="flex flex-col gap-7">
      <h1 className="text-[28px] leading-tight font-semibold tracking-[-0.015em]">Settings</h1>

      <Panel title="Protection">
        <Switch
          label="Kill switch"
          help="killSwitch"
          description={
            ksUnsupported
              ? `${supportNote(caps?.kill_switch, "The kill switch")}. While this is on, VPNFlow refuses to connect rather than connect without it.`
              : "Blocks all traffic outside the tunnel from Connect until you press Disconnect, also while reconnecting or if the app crashes. Server endpoints must be IP addresses. Change it while disconnected."
          }
          checked={settings.kill_switch}
          onChange={(v) => set({ kill_switch: v })}
        />
        <Switch
          label="Allow local network"
          help="allowLan"
          description="While the kill switch blocks, devices on your local network (printers, file shares, your router) stay reachable."
          checked={settings.kill_switch_allow_lan}
          disabled={!settings.kill_switch || ksUnsupported}
          onChange={(v) => set({ kill_switch_allow_lan: v })}
        />
        <div className="border-t border-line py-3 text-[13px]">
          <div className="font-medium">
            DNS and IPv6
            <InfoTip topic="dnsProtection" />
          </div>
          <p className="mt-0.5 max-w-[65ch] text-muted">
            {caps?.dns_protection.status === "supported"
              ? "Full-tunnel profiles send every DNS lookup to the profile's servers and IPv6 through the tunnel. If a profile has no IPv6, IPv6 is blocked while connected. VPNFlow re-checks this every few seconds; the dashboard shows the result."
              : (supportNote(caps?.dns_protection, "DNS protection") ?? "")}
          </p>
        </div>
      </Panel>

      <Panel title="Startup and window">
        <Switch
          label="Start VPNFlow when you sign in"
          checked={settings.launch_on_login}
          onChange={(v) => set({ launch_on_login: v })}
        />
        <Switch
          label="Connect on launch"
          help="connectOnLaunch"
          description="Connects the preferred profile when the app starts. Off by default."
          checked={settings.connect_on_launch}
          onChange={(v) => set({ connect_on_launch: v })}
          disabled={!settings.preferred_profile}
        />
        <div className="flex items-center justify-between border-t border-line py-3">
          <label htmlFor={prefId} className="font-medium">
            Preferred profile
            <InfoTip topic="preferredProfile" />
          </label>
          <select
            id={prefId}
            value={settings.preferred_profile ?? ""}
            onChange={(e) => set({ preferred_profile: e.target.value || null })}
            className="max-w-64 "
          >
            <option value="">None</option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
        <Switch
          label="Closing the window keeps VPNFlow in the tray"
          help="closeToTray"
          description={
            info?.tray_available
              ? "Closing the window never disconnects the VPN. Use Disconnect or Quit from the tray menu."
              : "No system tray was detected, so closing the window quits the app. An active tunnel stays up."
          }
          checked={settings.close_to_tray && !!info?.tray_available}
          disabled={!info?.tray_available}
          onChange={(v) => set({ close_to_tray: v })}
        />
      </Panel>

      <Panel title="Reconnection">
        <Switch
          label="Reconnect automatically"
          help="autoReconnect"
          description="If the tunnel drops, retry with increasing delays (1 s up to 60 s)."
          checked={settings.auto_reconnect}
          onChange={(v) => set({ auto_reconnect: v })}
        />
        <div className="flex items-center justify-between border-t border-line py-3">
          <label htmlFor={attemptsId} className="font-medium">
            Maximum attempts
          </label>
          <input
            id={attemptsId}
            type="number"
            min={1}
            max={50}
            value={settings.reconnect_max_attempts}
            onChange={(e) => set({ reconnect_max_attempts: Number(e.target.value) || 1 })}
            className="w-20 "
          />
        </div>
      </Panel>

      {caps?.backend === "macos_daemon" && (
        <Panel title="Trusted networks" help="trustedNetworks">
          <Switch
            label="Switch the VPN off on trusted networks, on elsewhere"
            description="Acts when you join a different network. Uses your preferred profile, or Quick connect if none is set."
            checked={settings.trusted_networks_enabled}
            onChange={(v) => set({ trusted_networks_enabled: v })}
          />
          <div className="py-3 text-[13px]">
            <form
              className="flex gap-2"
              onSubmit={async (e) => {
                e.preventDefault();
                const name = trustDraft.trim();
                if (!name) return;
                const id = await api.currentNetwork();
                if (!id) {
                  setTrustMsg("Could not tell which network this is. Connect to a network first.");
                  return;
                }
                if (settings.trusted_networks.some((n) => n.id === id)) {
                  setTrustMsg("This network is already trusted.");
                  return;
                }
                setTrustMsg(null);
                setTrustDraft("");
                void saveSettings({
                  ...settings,
                  trusted_networks: [...settings.trusted_networks, { id, name }],
                });
              }}
            >
              <input
                aria-label="Name for this network"
                value={trustDraft}
                onChange={(e) => setTrustDraft(e.target.value)}
                placeholder="Name, such as Home"
                className="w-64 "
              />
              <Button type="submit">Trust this network</Button>
            </form>
            {trustMsg && <p className="mt-2 text-muted">{trustMsg}</p>}
            <ul className="mt-2 flex flex-col gap-1">
              {settings.trusted_networks.map((n) => (
                <li key={n.id} className="flex items-center justify-between">
                  <span>{n.name}</span>
                  <Button
                    onClick={() =>
                      void set({ trusted_networks: settings.trusted_networks.filter((t) => t.id !== n.id) })
                    }
                  >
                    Remove
                  </Button>
                </li>
              ))}
            </ul>
          </div>
        </Panel>
      )}

      {caps?.backend === "macos_daemon" && (
        <Panel title="DNS and ad blocking" help="dns">
          <div className="py-3 text-[13px]">
            <label htmlFor={dnsId} className="font-medium">
              Name lookup service
            </label>
            <p className="mt-0.5 max-w-[65ch] text-muted">
              Every website name is looked up by a DNS service. Choosing a blocking one stops ads, trackers or
              dangerous sites before they load. The service you pick sees your lookups instead of the VPN
              provider, and DNS blocking does not catch every ad. Lookups always travel through the VPN.
              Applies from the next connection.
            </p>
            <select
              id={dnsId}
              value={dnsCustom ? "custom" : dnsPresetFor(settings.dns_servers)}
              onChange={(e) => {
                const v = e.target.value;
                if (v === "custom") {
                  setDnsCustom(true);
                  setDnsDraft(settings.dns_servers.join(", "));
                  return;
                }
                setDnsCustom(false);
                void set({ dns_servers: DNS_PRESETS.find((p) => p.id === v)?.servers ?? [] });
              }}
              className="mt-2 "
            >
              {DNS_PRESETS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
              <option value="custom">Your own servers…</option>
            </select>
            {!(dnsCustom || dnsPresetFor(settings.dns_servers) === "custom") && (
              <p className="mt-1.5 text-muted">
                {DNS_PRESETS.find((p) => p.id === dnsPresetFor(settings.dns_servers))?.note}
              </p>
            )}
            {(dnsCustom || dnsPresetFor(settings.dns_servers) === "custom") && (
              <form
                className="mt-2 flex gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  const list = dnsDraft
                    .split(/[\s,]+/)
                    .map((x) => x.trim())
                    .filter(Boolean);
                  // A rejected address shows up as an error toast and is not saved.
                  void saveSettings({ ...settings, dns_servers: list });
                }}
              >
                <input
                  id={dnsCustomId}
                  aria-label="DNS server addresses"
                  value={dnsDraft}
                  onChange={(e) => setDnsDraft(e.target.value)}
                  placeholder="94.140.14.14, 94.140.15.15"
                  className="w-72 "
                />
                <Button type="submit">Save</Button>
              </form>
            )}
          </div>
        </Panel>
      )}

      {caps?.backend === "macos_daemon" && (
        <Panel title="Skip the VPN" help="bypass">
          <div className="py-3 text-[13px]">
            <label htmlFor={bypassId} className="font-medium">
              Addresses that bypass the VPN
            </label>
            <p className="mt-0.5 max-w-[65ch] text-muted">
              Traffic to these addresses uses your normal connection, not the VPN. Add an address like
              192.168.1.20 or a range like 192.168.1.0/24. Applies from the next connection.
            </p>
            <form
              className="mt-2 flex gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                const entry = bypassDraft.trim();
                if (!entry) return;
                if (settings.bypass.includes(entry)) {
                  setBypassDraft("");
                  return;
                }
                // A rejected entry shows up as an error toast and is not added.
                void saveSettings({ ...settings, bypass: [...settings.bypass, entry] });
                setBypassDraft("");
              }}
            >
              <input
                id={bypassId}
                value={bypassDraft}
                onChange={(e) => setBypassDraft(e.target.value)}
                placeholder="192.168.1.20 or 10.0.0.0/24"
                className="w-64 "
              />
              <Button type="submit">Add</Button>
            </form>
            <ul className="mt-2 flex flex-col gap-1">
              {settings.bypass.map((entry) => (
                <li key={entry} className="flex items-center justify-between">
                  <code>{entry}</code>
                  <Button onClick={() => void set({ bypass: settings.bypass.filter((b) => b !== entry) })}>
                    Remove
                  </Button>
                </li>
              ))}
            </ul>
          </div>
        </Panel>
      )}

      <Panel title="Schedule" help="schedule">
        <Switch
          label="Connect and disconnect at set times"
          description="Uses your preferred profile, or Quick connect if none is set. It acts only at those times, so you can still switch manually in between."
          checked={settings.schedule_enabled}
          onChange={(v) => set({ schedule_enabled: v })}
        />
        {settings.schedule_enabled && (
          <div className="flex flex-col gap-3 border-t border-line py-3 text-[13px]">
            <div className="flex flex-wrap items-center gap-4">
              <label className="flex items-center gap-2 font-medium">
                Connect at
                <input
                  type="time"
                  value={settings.schedule_connect}
                  onChange={(e) => e.target.value && set({ schedule_connect: e.target.value })}
                  className=""
                />
              </label>
              <label className="flex items-center gap-2 font-medium">
                Disconnect at
                <input
                  type="time"
                  value={settings.schedule_disconnect}
                  onChange={(e) => e.target.value && set({ schedule_disconnect: e.target.value })}
                  className=""
                />
              </label>
            </div>
            <fieldset className="flex flex-wrap gap-3">
              <legend className="sr-only">Days</legend>
              {DAYS.map((name, i) => {
                const day = i + 1;
                const on = settings.schedule_days.includes(day);
                return (
                  <label key={name} className="flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      checked={on}
                      onChange={() =>
                        set({
                          schedule_days: on
                            ? settings.schedule_days.filter((d) => d !== day)
                            : [...settings.schedule_days, day],
                        })
                      }
                    />
                    {name}
                  </label>
                );
              })}
            </fieldset>
          </div>
        )}
      </Panel>

      <Panel title="Privacy">
        <Switch
          label="Connectivity check"
          help="connectivityCheck"
          checked={settings.connectivity_check}
          onChange={(v) => set({ connectivity_check: v })}
          description="While connected, look up example.com and whoami.akamai.net through your tunnel's DNS (at most once a minute). That confirms the resolver answers and reports its egress IPv4. This does not verify your VPN exit IP. The resolver sees those queries. On by default; turn off to skip them."
        />
        <Switch
          label="Notifications"
          checked={settings.notifications}
          onChange={(v) => set({ notifications: v })}
          description="Desktop alerts for connect, disconnect, and failure events when the system supports them."
        />
        <p className="border-t border-line py-3 text-[13px] text-muted">
          Automatic updates are not available yet — release signing has not been set up, so VPNFlow makes no
          update requests.
        </p>
        <p className="border-t border-line py-3 text-[13px] text-muted">
          VPNFlow collects no analytics and needs no account. Connectivity checks contact only the tunnel's
          configured DNS resolver (example.com reachability and whoami for public IP). Idle VPN servers are
          never probed.
        </p>
      </Panel>

      <Panel title="Appearance">
        <div className="flex items-center justify-between py-3">
          <label htmlFor={themeId} className="font-medium">
            Theme
          </label>
          <select
            id={themeId}
            value={settings.theme}
            onChange={(e) => set({ theme: e.target.value as Theme })}
            className=""
          >
            <option value="system">Match system</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </div>
      </Panel>
    </div>
  );
}
