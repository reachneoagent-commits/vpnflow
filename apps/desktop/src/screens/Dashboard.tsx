import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import type { ScreenId } from "../App";
import { Button, Dot, InfoTip, Panel, Row, type Tone } from "../components/ui";
import { api, asUiError } from "../lib/api";
import { featureText, featureTone, isActive, isBusy, stateHeadline, stateTone } from "../lib/connection";
import { ago, bytes, clock, duration, rate, routeLabel } from "../lib/format";
import type { HelpKey } from "../lib/help";
import { locationLabel } from "../lib/locations";
import { useStore } from "../lib/store";
import type { Check, CheckStatus, Feature, ProfileView, SpeedResult } from "../lib/types";
import { CountryFlag } from "./Locations";

function useNow(active: boolean) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [active]);
  return now;
}

export function Dashboard({ goTo }: { goTo: (s: ScreenId) => void }) {
  const { snapshot, profiles, settings, events, run, caps, countries } = useStore();
  const selectId = useId();
  const headId = useId();
  const state = snapshot?.state ?? { state: "disconnected" as const };
  const active = isActive(state);
  const now = useNow(state.state === "connected");
  const lastUsed = profiles
    .filter((p) => p.server.last_connected_ms !== null)
    .sort((a, b) => (b.server.last_connected_ms ?? 0) - (a.server.last_connected_ms ?? 0))[0];
  const fallback = lastUsed ?? profiles.find((p) => p.preferred) ?? profiles[0];
  const [chosen, setChosen] = useState<string | undefined>(undefined);
  const selected: ProfileView | undefined =
    snapshot?.profile ?? profiles.find((p) => p.id === chosen) ?? fallback;

  if (profiles.length === 0) {
    return (
      <div className="max-w-[46ch] py-14">
        <h1 className="text-[34px] leading-[1.1] font-semibold tracking-[-0.015em]">
          Import a WireGuard file to get started
        </h1>
        <p className="mt-3 text-[15px] text-muted">
          Drop a <span className="font-mono">.conf</span> onto this window, or import one from your VPN
          provider or server.
        </p>
        <div className="mt-7 flex gap-2">
          <Button variant="primary" onClick={() => run(api.importDialog)}>
            Import profile
          </Button>
          <Button onClick={() => goTo("locations")}>Add server</Button>
        </div>
      </div>
    );
  }

  const stats = snapshot?.stats;
  const hasHandshakeTime = caps?.exact_handshake_time.status === "supported";
  const peerTime = hasHandshakeTime ? stats?.last_handshake_ms : stats?.last_response_ms;
  const connected = state.state === "connected";
  const s = selected?.summary;
  const peerConfirmed = connected && state.peer === "confirmed";

  const uncertain = state.state === "uncertain";
  const tunnelTone: Tone = connected ? "ok" : uncertain ? "bad" : active ? "pending" : "none";
  const serverTone: Tone = connected ? stateTone(state) : "none";
  const reach = snapshot?.reachability ?? { status: "off" as const };
  const reachTone: HopTone =
    reach.status === "verified" ? "ok" : reach.status === "failed" ? "bad" : "untested";
  const reachDetail =
    reach.status === "verified"
      ? "Tunnel DNS answered"
      : reach.status === "failed"
        ? reach.detail
        : reach.status === "unsupported"
          ? `Not tested: ${reach.detail}`
          : connected && settings?.connectivity_check
            ? "Checking"
            : "Not tested";

  return (
    <div className="flex flex-col gap-9">
      <section aria-labelledby={headId} className="flex flex-col gap-7">
        <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4">
          <div className="min-w-0">
            <h1
              id={headId}
              data-tone={peerConfirmed ? "ok" : "idle"}
              className="dashboard-headline text-[34px] leading-[1.1] font-semibold tracking-[-0.015em] text-balance"
              aria-live="polite"
            >
              {stateHeadline(state, selected?.name)}
            </h1>
            <p
              className={`mt-2 max-w-[62ch] text-[14px] ${state.state === "failed" || uncertain ? "text-danger" : "text-muted"}`}
            >
              {state.state === "failed" || state.state === "uncertain"
                ? state.reason
                : state.state === "reconnecting"
                  ? `Retrying, attempt ${state.attempt}`
                  : statusSub(state.state)}
              {connected && ` for ${duration(now - state.since_ms)}`}
            </p>
            {!active && snapshot?.protection.kill_switch.status === "verified" && (
              <p role="status" className="mt-1 max-w-[62ch] text-[14px] font-semibold text-pending">
                The kill switch is blocking ordinary traffic outside the tunnel.
              </p>
            )}
          </div>
          <div className="flex items-center gap-2 pt-1">
            <PrimaryAction selected={selected} />
          </div>
        </header>

        <ol aria-label="What is confirmed" className="grid grid-cols-4 gap-x-1.5">
          <Hop tone={tunnelTone} title="This device" detail={s ? (s.addresses[0] ?? "") : ""} mono />
          <Hop
            tone={tunnelTone}
            title="Tunnel interface"
            detail={
              connected
                ? (stats?.interface ?? "Up")
                : uncertain
                  ? "Status unknown"
                  : active
                    ? "Starting"
                    : "Not started"
            }
            mono={connected && !!stats?.interface}
          />
          <Hop
            tone={serverTone}
            title="Server response"
            detail={
              !connected
                ? "Not started"
                : state.peer === "confirmed"
                  ? "Server answered"
                  : state.peer === "no_response"
                    ? "No reply from the server. If this just started, the network may be down or switched; VPNFlow confirms again when the server answers. If it never connected, check the endpoint, keys and firewall."
                    : "Waiting for data from the server"
            }
          />
          <Hop tone={reachTone} title="Internet reachability" detail={reachDetail} />
        </ol>

        <ul
          aria-label="Protection"
          className="grid gap-px overflow-hidden rounded-xl border border-line bg-line sm:grid-cols-3"
        >
          <Guard title="Kill switch" help="killSwitch" f={snapshot?.protection.kill_switch} />
          <Guard title="DNS leak protection" help="dnsProtection" f={snapshot?.protection.dns} />
          <Guard title="IPv6 protection" help="ipv6Protection" f={snapshot?.protection.ipv6} />
        </ul>

        <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
          <div className="flex min-w-0 items-center gap-3">
            <CountryFlag code={selected?.server.location?.country_code} />
            <div>
              <p className="font-semibold">
                {selected ? locationLabel(selected, countries) : "No location selected"}
              </p>
              <p className="text-xs text-muted">
                {connected ? "Active WireGuard profile" : "Selected WireGuard profile"}
              </p>
              <p className="text-xs text-muted">{selected?.name}</p>
              {selected?.server.location && <p className="text-xs text-muted">User-assigned location</p>}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <label htmlFor={selectId} className="sr-only">
              Profile
            </label>
            <select
              id={selectId}
              value={selected?.id}
              disabled={active}
              onChange={(e) => setChosen(e.target.value)}
              className="h-10 max-w-56 rounded-lg border border-line bg-surface px-3 text-[14px] disabled:opacity-60"
            >
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                  {p.id === settings?.preferred_profile ? " (preferred)" : ""}
                </option>
              ))}
            </select>
            <Button
              disabled={isBusy(state) || state.state === "reconnecting" || state.state === "uncertain"}
              onClick={() => run(api.quickConnect)}
            >
              Quick Connect
            </Button>
            <InfoTip topic="quickConnect" />
            <Button onClick={() => goTo("locations")}>Change location</Button>
          </div>
        </div>
      </section>

      <Panel title="Connection details">
        <dl className="grid gap-x-10 sm:grid-cols-2">
          <div>
            <Row label="Endpoint">
              <span className="font-mono text-[13px]">{s?.endpoints[0] ?? "None"}</span>
            </Row>
            <Row label="Tunnel address">
              <span className="font-mono text-[13px]">{s?.addresses.join(", ")}</span>
            </Row>
            <Row label="Routing">{s ? routeLabel[s.route_kind] : ""}</Row>
            <Row
              label={hasHandshakeTime ? "Last handshake" : "Last server response"}
              state={peerTime ? "confirmed" : connected ? "pending" : "unavailable"}
            >
              {peerTime
                ? ago(peerTime, now)
                : connected
                  ? hasHandshakeTime
                    ? "None yet"
                    : state.peer === "no_response"
                      ? "No response yet"
                      : "Waiting for the server"
                  : undefined}
            </Row>
            <PublicIpRow
              connected={connected}
              f={snapshot?.public_ip}
              countryCode={selected?.server.location?.country_code}
            />
          </div>
          <div>
            <Row label="Received" state={stats ? "confirmed" : "unavailable"}>
              {stats ? bytes(stats.rx_bytes) : undefined}
            </Row>
            <Row label="Sent" state={stats ? "confirmed" : "unavailable"}>
              {stats ? bytes(stats.tx_bytes) : undefined}
            </Row>
            <Row label="Download" state={stats ? "confirmed" : "unavailable"}>
              {stats ? rate(stats.rx_rate_bps) : undefined}
            </Row>
            <Row label="Upload" state={stats ? "confirmed" : "unavailable"}>
              {stats ? rate(stats.tx_rate_bps) : undefined}
            </Row>
            <Row label="Duration" state={connected ? "confirmed" : "unavailable"}>
              {connected ? duration(now - state.since_ms) : "Not connected"}
            </Row>
          </div>
        </dl>
        {s && s.warnings.length > 0 && (
          <ul className="my-3 rounded-lg bg-pending-soft px-3 py-2 text-[13px] text-pending">
            {s.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        )}
      </Panel>

      <LeakTestPanel connected={connected} />

      <SpeedPanel connected={connected} />

      <Panel
        title="Recent events"
        actions={
          <Button variant="ghost" className="py-0.5 text-[12px]" onClick={() => goTo("activity")}>
            View all
          </Button>
        }
      >
        {events.length === 0 ? (
          <p className="py-3 text-muted">Nothing yet.</p>
        ) : (
          <ul>
            {events
              .slice(-5)
              .reverse()
              .map((e, i) => (
                <li
                  key={`${e.at_ms}-${i}`}
                  className="flex items-baseline gap-3 border-b border-line py-2 last:border-b-0"
                >
                  <span className="w-20 shrink-0 font-mono text-[12.5px] text-muted">{clock(e.at_ms)}</span>
                  <span
                    className={e.level === "error" ? "text-danger" : e.level === "warn" ? "text-pending" : ""}
                  >
                    {e.message}
                  </span>
                </li>
              ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}

function statusSub(s: string) {
  return {
    disconnected: "Idle",
    preparing: "Checking profile",
    connecting: "Bringing the tunnel up",
    connected: "Active",
    disconnecting: "Removing the tunnel",
    failed: "Stopped",
    uncertain: "May still be active",
    reconnecting: "Retrying",
  }[s];
}

const big = "h-10 min-w-32 rounded-lg px-5 text-[14px]";

function PrimaryAction({ selected }: { selected?: ProfileView }) {
  const { snapshot, run } = useStore();
  const state = snapshot?.state ?? { state: "disconnected" as const };
  if (state.state === "connecting" || state.state === "preparing" || state.state === "reconnecting") {
    return (
      <Button className={big} onClick={() => run(api.disconnect)}>
        Cancel
      </Button>
    );
  }
  if (isActive(state)) {
    return (
      <Button
        variant="secondary"
        className={big}
        disabled={isBusy(state)}
        onClick={() => run(api.disconnect)}
      >
        Disconnect
      </Button>
    );
  }
  const blocking = snapshot?.protection.kill_switch.status === "verified";
  return (
    <>
      {blocking && (
        <Button className={big} onClick={() => run(api.disconnect)}>
          Stop blocking
        </Button>
      )}
      <ConnectButton selected={selected} />
    </>
  );
}

function ConnectButton({ selected }: { selected?: ProfileView }) {
  const { run } = useStore();
  return (
    <Button
      variant="primary"
      className={big}
      disabled={!selected?.server.enabled || selected.server.registration_pending}
      title={
        selected?.server.registration_pending
          ? "Register the client identity on your server, then confirm registration in Locations"
          : undefined
      }
      onClick={() => selected && run(() => api.connect(selected.id))}
    >
      Connect
    </Button>
  );
}

type HopTone = Tone | "untested";

/** One hop of the tunnel path. The bar leading into it is lit only when that hop is confirmed. */
function Hop({
  tone,
  title,
  detail,
  mono,
}: {
  tone: HopTone;
  title: string;
  detail: string;
  mono?: boolean;
}) {
  return (
    <li data-tone={tone} className="hop flex min-w-0 flex-col gap-2.5">
      <div className="hop-line" aria-hidden />
      <div className="flex items-center gap-2">
        <Dot tone={tone === "untested" ? "none" : tone} />
        <span className="truncate text-[14px] font-semibold">{title}</span>
      </div>
      <span
        className={`text-[13px] leading-snug break-words ${tone === "bad" ? "text-danger" : "text-muted"} ${
          mono ? "font-mono" : ""
        }`}
      >
        {detail}
      </span>
    </li>
  );
}

function Guard({ title, help, f }: { title: string; help: HelpKey; f?: Feature }) {
  const feature = f ?? { status: "off" as const };
  const tone = featureTone(feature);
  return (
    <li className="flex flex-col gap-1 bg-surface px-4 py-3">
      <span className="text-[13px] text-muted">
        {title}
        <InfoTip topic={help} />
      </span>
      <span className="flex items-center gap-2 font-semibold">
        <Dot tone={tone} />
        <span className={tone === "bad" ? "text-danger" : ""}>{featureText(feature)}</span>
      </span>
      {"detail" in feature && <span className="text-[12.5px] leading-snug text-muted">{feature.detail}</span>}
    </li>
  );
}

function countryName(code: string | undefined) {
  if (!code) return undefined;
  try {
    return new Intl.DisplayNames(undefined, { type: "region" }).of(code.toUpperCase());
  } catch {
    return undefined;
  }
}

function PublicIpRow({
  connected,
  f,
  countryCode,
}: {
  connected: boolean;
  f?: Feature;
  countryCode?: string;
}) {
  const feature = f ?? { status: "off" as const };
  const state =
    feature.status === "verified" ? "confirmed" : feature.status === "configured" ? "pending" : "unavailable";
  let body: ReactNode;
  if (feature.status === "verified") {
    const country = countryName(countryCode);
    body = (
      <span>
        <span className="font-mono text-[13px]">{feature.detail}</span>
        {country ? <span className="text-muted"> · {country} (server's listed country)</span> : null}
      </span>
    );
  } else if (feature.status === "configured") {
    body = feature.detail;
  } else if (feature.status === "failed" || feature.status === "unsupported") {
    body = <span className="text-danger">{feature.detail}</span>;
  } else if (connected) {
    body = "Enable connectivity check in Settings";
  } else {
    body = undefined;
  }
  return (
    <Row label="DNS resolver egress IPv4" state={state}>
      {body}
    </Row>
  );
}

const mbps = (n: number | null) => (n === null ? "Not measured" : `${n.toFixed(1)} Mbps`);
const ms = (n: number | null) => (n === null ? "Not measured" : `${Math.round(n)} ms`);

/** Runs only when pressed: it downloads and uploads test data and contacts Cloudflare. */
function SpeedPanel({ connected }: { connected: boolean }) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<(SpeedResult & { at: number }) | null>(null);
  const [error, setError] = useState<string | null>(null);

  // A result belongs to the connection it was measured on.
  useEffect(() => {
    if (!connected) {
      setResult(null);
      setError(null);
    }
  }, [connected]);

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult({ ...(await api.speedTest()), at: Date.now() });
    } catch (e) {
      setError(asUiError(e).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Panel
      title="Connection speed"
      help="speedTest"
      actions={
        <Button className="min-w-28 whitespace-nowrap" onClick={start} disabled={!connected || busy}>
          {busy ? (
            <>
              <span
                aria-hidden
                className="size-3 animate-spin rounded-full border-2 border-current border-t-transparent"
              />
              Testing
            </>
          ) : result ? (
            "Test again"
          ) : (
            "Test speed"
          )}
        </Button>
      }
    >
      {!connected ? (
        <p className="py-2 text-[13px] text-muted">Connect to a server to test its speed.</p>
      ) : (
        <dl>
          <Row label="Download" state={result ? "confirmed" : "unavailable"}>
            {result ? mbps(result.download_mbps) : undefined}
          </Row>
          <Row label="Upload" state={result ? "confirmed" : "unavailable"}>
            {result ? mbps(result.upload_mbps) : undefined}
          </Row>
          <Row label="Latency" state={result ? "confirmed" : "unavailable"}>
            {result ? ms(result.latency_ms) : undefined}
          </Row>
          <Row label="Jitter" state={result ? "confirmed" : "unavailable"}>
            {result ? ms(result.jitter_ms) : undefined}
          </Row>
        </dl>
      )}
      {error && (
        <p role="alert" className="mt-2 text-[13px] text-danger">
          {error}
        </p>
      )}
      {result?.notes.map((n) => (
        <p key={n} className="mt-2 text-[13px] text-pending">
          {n}
        </p>
      ))}
      <p className="mt-2 text-[12px] text-muted">
        {result
          ? `Measured through your VPN with ${result.provider} at ${clock(result.at)}. Results vary by server and time of day.`
          : "Uses about 30 MB of data and contacts Cloudflare's speed server only when you press the button."}
      </p>
    </Panel>
  );
}

const LEAK_IDS = ["kill_switch", "dns", "ipv6", "reachability", "public_ip", "other_vpn", "browser_dns"];
const leakTone: Record<CheckStatus, Tone> = { pass: "ok", warn: "pending", fail: "bad", skipped: "none" };
const leakText: Record<CheckStatus, string> = {
  pass: "Protected",
  warn: "Check this",
  fail: "Leaking",
  skipped: "Not tested",
};

function LeakTestPanel({ connected }: { connected: boolean }) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ checks: Check[]; at: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // A result belongs to the connection it was measured on.
  useEffect(() => {
    if (!connected) {
      setResult(null);
      setError(null);
    }
  }, [connected]);

  // Check automatically a few seconds after connecting, so the user sees their
  // protection without having to ask. "Test again" still reruns it by hand.
  const startRef = useRef<() => void>(() => {});
  useEffect(() => {
    if (!connected) return;
    const t = setTimeout(() => startRef.current(), 4000);
    return () => clearTimeout(t);
  }, [connected]);

  const [stopping, setStopping] = useState(false);
  const [stopNote, setStopNote] = useState<string | null>(null);

  const stopOthers = async () => {
    setStopping(true);
    setStopNote(null);
    try {
      const r = await api.stopOtherVpns();
      setStopNote(r.map((x) => `${x.name}: ${x.message}`).join("; ") || "Nothing to stop.");
      // Give the app a moment to quit, then check again.
      await new Promise((ok) => setTimeout(ok, 1500));
      await start();
    } catch (e) {
      setError(asUiError(e).message);
    } finally {
      setStopping(false);
    }
  };

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      const all = await api.diagnostics();
      setResult({ checks: all.filter((c) => LEAK_IDS.includes(c.id)), at: Date.now() });
    } catch (e) {
      setError(asUiError(e).message);
    } finally {
      setBusy(false);
    }
  };

  startRef.current = () => {
    void start();
  };

  const worst = result?.checks.some((c) => c.status === "fail")
    ? "Something is leaking. See below."
    : result?.checks.some((c) => c.status === "warn")
      ? "No leak found, but one check needs a look."
      : "No leaks found.";

  return (
    <Panel
      title="Leak test"
      help="leakTest"
      actions={
        <Button className="min-w-28 whitespace-nowrap" onClick={start} disabled={!connected || busy}>
          {busy ? (
            <>
              <span
                aria-hidden
                className="size-3 animate-spin rounded-full border-2 border-current border-t-transparent"
              />
              Testing
            </>
          ) : result ? (
            "Test again"
          ) : (
            "Run leak test"
          )}
        </Button>
      }
    >
      {!connected ? (
        <p className="py-2 text-[13px] text-muted">Connect to a server to run a leak test.</p>
      ) : result ? (
        <>
          <p className="pb-1 text-[13px] font-medium">{worst}</p>
          <ul>
            {result.checks.map((c) => (
              <li key={c.id} className="flex items-start gap-3 border-b border-line py-2 last:border-b-0">
                <span className="pt-1.5">
                  <Dot tone={leakTone[c.status]} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex justify-between gap-3">
                    <span className="font-medium">{c.title}</span>
                    <span className="text-[12.5px] text-muted">{leakText[c.status]}</span>
                  </div>
                  <p className="text-[12.5px] break-words text-muted">{c.detail}</p>
                  {c.id === "other_vpn" && c.status === "warn" && (
                    <div className="mt-1.5">
                      <Button onClick={stopOthers} disabled={stopping || busy}>
                        {stopping ? "Stopping…" : "Stop it"}
                      </Button>
                      {stopNote && <p className="mt-1 text-[12px] text-muted">{stopNote}</p>}
                    </div>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </>
      ) : (
        <p className="py-2 text-[13px] text-muted">
          Checks run by themselves a few seconds after you connect.
        </p>
      )}
      {error && (
        <p role="alert" className="mt-2 text-[13px] text-danger">
          {error}
        </p>
      )}
      <p className="mt-2 text-[12px] text-muted">
        {result
          ? `Checked at ${clock(result.at)}. Tests run on this device and contact no third-party site.`
          : "Runs on this device and contacts no third-party site."}
      </p>
    </Panel>
  );
}
