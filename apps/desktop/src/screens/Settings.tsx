import { useId, useState } from "react";
import { Button, InfoTip, Panel, Switch } from "../components/ui";
import { useStore } from "../lib/store";
import type { Settings, Support, Theme } from "../lib/types";

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
  const [bypassDraft, setBypassDraft] = useState("");
  if (!settings) return null;
  const set = (patch: Partial<Settings>) => saveSettings({ ...settings, ...patch });
  const ksUnsupported = caps?.kill_switch.status !== "supported";

  return (
    <div className="flex max-w-3xl flex-col gap-7">
      <h1 className="text-[28px] leading-tight font-semibold tracking-[-0.015em]">Settings</h1>

      <Panel title="Appearance">
        <div className="flex items-center justify-between py-3">
          <label htmlFor={themeId} className="font-medium">
            Theme
          </label>
          <select
            id={themeId}
            value={settings.theme}
            onChange={(e) => set({ theme: e.target.value as Theme })}
            className="rounded-md border border-line bg-surface px-2.5 py-1.5 text-[13px]"
          >
            <option value="system">Match system</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
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
            className="max-w-64 rounded-md border border-line bg-surface px-2.5 py-1.5 text-[13px]"
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
            className="w-20 rounded-md border border-line bg-surface px-2.5 py-1.5 text-[13px]"
          />
        </div>
      </Panel>

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
                className="w-64 rounded-md border border-line bg-surface px-2.5 py-1.5 text-[13px]"
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

      <Panel title="Setup">
        <div className="flex items-center justify-between gap-4 py-3">
          <p className="text-[13px] text-muted">
            Walk through account, server files and country choices again. Your current servers stay.
          </p>
          <Button onClick={() => window.dispatchEvent(new Event("vpnflow:setup"))}>Run setup again</Button>
        </div>
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
    </div>
  );
}
