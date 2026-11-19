import {
  Cable,
  Check as CheckIcon,
  ChevronDown,
  Compass,
  Globe,
  Laptop,
  Layers,
  MapPin,
  Network,
  Power,
  Radio,
  Server,
  ShieldCheck,
  Star,
} from "lucide-react";
import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import type { ScreenId } from "../App";
import deskArt from "../assets/empty-desk.svg?raw";
import { Button, InfoTip, Panel, Row, type Tone } from "../components/ui";
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

type SpeedRun = {
  busy: boolean;
  result: (SpeedResult & { at: number }) | null;
  error: string | null;
  start: () => Promise<void>;
};

/** One speed test per connection: runs by itself once the link is stable (if allowed), or on demand. */
function useSpeedTest(
  connected: boolean,
  stable: boolean,
  auto: boolean,
  session: number | undefined,
): SpeedRun {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<(SpeedResult & { at: number }) | null>(null);
  const [error, setError] = useState<string | null>(null);
  const autoRanFor = useRef<number | undefined>(undefined);

  // A result belongs to the connection it was measured on.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new session clears the old result
  useEffect(() => {
    setResult(null);
    setError(null);
  }, [connected, session]);

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

  // biome-ignore lint/correctness/useExhaustiveDependencies: start is stable enough; trigger is the session
  useEffect(() => {
    if (auto && connected && stable && session !== undefined && autoRanFor.current !== session) {
      autoRanFor.current = session;
      void start();
    }
  }, [auto, connected, stable, session]);

  return { busy, result, error, start };
}

export function Dashboard({ goTo }: { goTo: (s: ScreenId) => void }) {
  const { snapshot, profiles, settings, events, run, caps } = useStore();
  const headId = useId();
  const state = snapshot?.state ?? { state: "disconnected" as const };
  const active = isActive(state);
  const now = useNow(state.state === "connected");
  const speed = useSpeedTest(
    state.state === "connected",
    state.state === "connected" && state.peer === "confirmed" && now - state.since_ms >= 8000,
    settings?.auto_speed_test ?? false,
    state.state === "connected" ? state.since_ms : undefined,
  );
  const lastUsed = profiles
    .filter((p) => p.server.last_connected_ms !== null)
    .sort((a, b) => (b.server.last_connected_ms ?? 0) - (a.server.last_connected_ms ?? 0))[0];
  const fallback = lastUsed ?? profiles.find((p) => p.preferred) ?? profiles[0];
  const [chosen, setChosen] = useState<string | undefined>(undefined);
  const selected: ProfileView | undefined =
    snapshot?.profile ?? profiles.find((p) => p.id === chosen) ?? fallback;

  if (profiles.length === 0) {
    return (
      <div className="flex items-center gap-10 py-10">
        <div className="max-w-[46ch] flex-1">
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
        <aside aria-hidden className="hidden shrink-0 flex-col items-center gap-3 min-[760px]:flex">
          <div
            className="ill w-full max-w-[280px]"
            // biome-ignore lint/security/noDangerouslySetInnerHtml: bundled static SVG file, no user input
            dangerouslySetInnerHTML={{ __html: deskArt }}
          />
          <p className="max-w-[24ch] text-center text-[13px] leading-snug text-muted">
            Nothing here yet. Add a server file and you are ready.
          </p>
        </aside>
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
  const reachTone: Tone = reach.status === "verified" ? "ok" : reach.status === "failed" ? "bad" : "none";
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
  const checks: {
    untested?: boolean;
    tone: Tone;
    title: string;
    detail: string;
    icon: ReactNode;
    mono?: boolean;
  }[] = [
    {
      tone: tunnelTone,
      title: "This device",
      detail: s ? (s.addresses[0] ?? "") : "",
      icon: <Laptop size={20} aria-hidden />,
      mono: true,
    },
    {
      tone: tunnelTone,
      title: "Tunnel interface",
      detail: connected
        ? (stats?.interface ?? "Up")
        : uncertain
          ? "Status unknown"
          : active
            ? "Starting"
            : "Not started",
      icon: <Cable size={20} aria-hidden />,
      mono: connected && !!stats?.interface,
    },
    {
      tone: serverTone,
      title: "Server response",
      detail: !connected
        ? "Not started"
        : state.peer === "confirmed"
          ? "Server answered"
          : state.peer === "no_response"
            ? "No reply from the server. If this just started, the network may be down or switched; VPNFlow confirms again when the server answers. If it never connected, check the endpoint, keys and firewall."
            : "Waiting for data from the server",
      icon: <Server size={20} aria-hidden />,
    },
    {
      untested: reachTone === "none",
      tone: reachTone,
      title: "Internet reachability",
      detail: reachDetail,
      icon: <Globe size={20} aria-hidden />,
    },
  ];
  const confirmedCount = checks.filter((c) => c.tone === "ok").length;
  const heroState = connected ? "connected" : active ? "connecting" : "idle";

  return (
    <div className="flex flex-col gap-6">
      <section aria-labelledby={headId} data-state={heroState}>
        <div className="hero">
          <div className="hero-main flex flex-col items-center px-6 py-7 text-center">
            <span className="inline-flex items-center gap-2 rounded-full bg-white/10 px-3 py-1 text-[12px] font-semibold">
              <span className={`size-2 rounded-full ${peerConfirmed ? "bg-(--h-acc)" : "bg-(--h-mute)"}`} />
              {peerConfirmed ? "Connection confirmed" : active ? "In progress" : "Idle"}
            </span>
            <h1
              id={headId}
              data-tone={peerConfirmed ? "ok" : "idle"}
              className="mt-3 mb-1.5 text-[26px] leading-tight font-semibold tracking-[-0.02em] text-balance"
              aria-live="polite"
            >
              {stateHeadline(state, selected?.name)}
            </h1>
            <p
              className={`min-h-10 max-w-[300px] text-[13px] ${state.state === "failed" || uncertain ? "text-[#f0786a]" : "text-(--h-mute)"}`}
            >
              {state.state === "failed" || state.state === "uncertain"
                ? state.reason
                : state.state === "reconnecting"
                  ? `Retrying, attempt ${state.attempt}`
                  : statusSub(state.state)}
              {connected && ` for ${duration(now - state.since_ms)}`}
            </p>
            {!active && snapshot?.protection.kill_switch.status === "verified" && (
              <p role="status" className="mt-1 max-w-[300px] text-[13px] font-semibold text-(--h-pending)">
                The kill switch is blocking ordinary traffic outside the tunnel.
              </p>
            )}
            <Orb selected={selected} />
            {connected && (
              <p className="mb-5 font-mono text-[clamp(26px,3vw,34px)] leading-none tabular-nums">
                {duration(now - state.since_ms)}
              </p>
            )}
            <LocationPicker
              profiles={profiles}
              selected={selected}
              disabled={active}
              onPick={setChosen}
              preferredId={settings?.preferred_profile ?? null}
            />
            <div className="mt-3 flex flex-wrap items-center justify-center gap-2">
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

          <div className="flex min-w-0 flex-col gap-5 p-6">
            <div className="flex justify-between border-b border-(--h-line) pb-2.5 text-[15px] font-bold">
              <h2>Connection check</h2>
              <span className="text-[13px] font-medium text-(--h-acc)">{confirmedCount} of 4 confirmed</span>
            </div>
            <ol aria-label="What is confirmed" className="-mt-2">
              {checks.map((c, i) => (
                <li
                  key={c.title}
                  data-tone={c.untested ? "untested" : c.tone}
                  data-s={
                    c.tone === "ok" ? "done" : c.tone === "pending" ? "work" : c.tone === "bad" ? "bad" : ""
                  }
                  className={`chk${i === checks.length - 1 ? " last" : ""}`}
                >
                  <span className="cn" aria-hidden />
                  <span className="tile">{c.icon}</span>
                  <span className="min-w-0 text-[14.5px] font-bold">{c.title}</span>
                  <span className={`v break-words ${c.mono ? "font-mono" : ""}`}>{c.detail}</span>
                </li>
              ))}
            </ol>

            <div className="rounded-xl bg-(--h-fill) px-5.5 py-5">
              <h2 className="mb-3.5 text-[15px] font-bold">Connection details</h2>
              <dl className="grid grid-cols-2 gap-x-5 gap-y-4 sm:grid-cols-3">
                <Detail label="Endpoint">
                  <span className="font-mono text-[13px]">{s?.endpoints[0] ?? "None"}</span>
                </Detail>
                <Detail label="Tunnel address">
                  <span className="font-mono text-[13px]">{s?.addresses.join(", ")}</span>
                </Detail>
                <Detail label="Routing">{s ? routeLabel[s.route_kind] : ""}</Detail>
                <Detail
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
                </Detail>
                <PublicIpRow
                  connected={connected}
                  f={snapshot?.public_ip}
                  countryCode={selected?.server.location?.country_code}
                />
                <Detail label="Received" state={stats ? "confirmed" : "unavailable"}>
                  {stats ? bytes(stats.rx_bytes) : undefined}
                </Detail>
                <Detail label="Sent" state={stats ? "confirmed" : "unavailable"}>
                  {stats ? bytes(stats.tx_bytes) : undefined}
                </Detail>
                <Detail label="Current traffic in" state={stats ? "confirmed" : "unavailable"}>
                  {stats ? rate(stats.rx_rate_bps) : undefined}
                </Detail>
                <Detail label="Current traffic out" state={stats ? "confirmed" : "unavailable"}>
                  {stats ? rate(stats.tx_rate_bps) : undefined}
                </Detail>
                <Detail
                  label="Server download speed"
                  state={speed.result ? "confirmed" : connected ? "pending" : "unavailable"}
                >
                  {speed.result
                    ? mbps(speed.result.download_mbps)
                    : speed.busy
                      ? "Measuring…"
                      : connected
                        ? settings?.auto_speed_test
                          ? "Measured once stable"
                          : "Press Test speed below"
                        : undefined}
                </Detail>
                <Detail
                  label="Server upload speed"
                  state={speed.result ? "confirmed" : connected ? "pending" : "unavailable"}
                >
                  {speed.result
                    ? mbps(speed.result.upload_mbps)
                    : speed.busy
                      ? "Measuring…"
                      : connected
                        ? settings?.auto_speed_test
                          ? "Measured once stable"
                          : "Press Test speed below"
                        : undefined}
                </Detail>
                <Detail label="Duration" state={connected ? "confirmed" : "unavailable"}>
                  {connected ? duration(now - state.since_ms) : "Not connected"}
                </Detail>
              </dl>
              {s && s.warnings.length > 0 && (
                <ul className="mt-4 rounded-lg bg-(--h-pending)/15 px-3 py-2 text-[13px] text-(--h-pending)">
                  {s.warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        </div>
        <LeakSection connected={connected} protection={snapshot?.protection} goTo={goTo} />
      </section>

      <SpeedPanel connected={connected} speed={speed} auto={settings?.auto_speed_test ?? false} />

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

/** Key/value cell inside the hero's details card. */
function Detail({
  label,
  children,
  state = "confirmed",
}: {
  label: string;
  children?: ReactNode;
  state?: "confirmed" | "pending" | "unavailable";
}) {
  return (
    <div className="min-w-0">
      <dt className="text-[12px] text-(--h-mute)">{label}</dt>
      <dd
        className={`text-[13.5px] font-semibold break-words ${
          state === "pending" ? "text-(--h-pending)" : state === "unavailable" ? "text-(--h-mute)" : ""
        }`}
      >
        {state === "pending" ? (children ?? "Pending") : (children ?? "Unavailable")}
      </dd>
    </div>
  );
}

/** The power orb: one control for Connect, Cancel and Disconnect. */
function Orb({ selected }: { selected?: ProfileView }) {
  const { snapshot, run } = useStore();
  const state = snapshot?.state ?? { state: "disconnected" as const };
  const blocking = !isActive(state) && snapshot?.protection.kill_switch.status === "verified";
  let label = "Connect";
  let disabled = !selected?.server.enabled || !!selected.server.registration_pending;
  let onClick = () => selected && run(() => api.connect(selected.id));
  if (state.state === "connecting" || state.state === "preparing" || state.state === "reconnecting") {
    label = "Cancel";
    disabled = false;
    onClick = () => run(api.disconnect);
  } else if (isActive(state)) {
    label = "Disconnect";
    disabled = isBusy(state);
    onClick = () => run(api.disconnect);
  }
  return (
    <>
      <button
        type="button"
        className="orb"
        disabled={disabled}
        onClick={onClick}
        title={
          label === "Connect" && selected?.server.registration_pending
            ? "Register the client identity on your server, then confirm registration in Locations"
            : undefined
        }
      >
        <span className="ring r3" aria-hidden />
        <span className="ring r2" aria-hidden />
        <span className="ring r1" aria-hidden />
        <span className="core">
          <Power aria-hidden />
          {label}
        </span>
      </button>
      {blocking && (
        <Button className="mb-4" onClick={() => run(api.disconnect)}>
          Stop blocking
        </Button>
      )}
    </>
  );
}

/** Location dropdown. Locked while a tunnel is active: switching means disconnecting first. */
function LocationPicker({
  profiles,
  selected,
  disabled,
  onPick,
  preferredId,
}: {
  profiles: ProfileView[];
  selected?: ProfileView;
  disabled: boolean;
  onPick: (id: string) => void;
  preferredId: string | null;
}) {
  const { countries } = useStore();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const listId = useId();
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);
  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);
  const ordered = [...profiles].sort((a, b) => Number(b.server.favorite) - Number(a.server.favorite));
  return (
    <div ref={ref} className="relative mt-4 w-[min(100%,330px)] text-left">
      <button
        type="button"
        aria-haspopup="true"
        aria-expanded={open}
        aria-controls={listId}
        aria-label="Location"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className="flex h-12 w-full items-center gap-2.5 rounded-xl border border-transparent bg-(--h-fill) px-3.5 text-left text-[13.5px] font-semibold hover:enabled:border-(--h-line) disabled:cursor-not-allowed disabled:opacity-60"
      >
        <CountryFlag code={selected?.server.location?.country_code} />
        <span className="min-w-0 flex-1 truncate">
          {selected ? locationLabel(selected, countries) : "No location selected"}
          {selected && <span className="ml-2 text-[12px] font-medium text-(--h-mute)">{selected.name}</span>}
        </span>
        <ChevronDown
          size={18}
          aria-hidden
          className={`shrink-0 text-(--h-acc) transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      <p className="mt-2 min-h-[18px] text-[12px] text-(--h-mute)">
        {disabled ? "Disconnect to change location." : "Pick any location, then press Connect."}
      </p>
      {open && (
        <ul
          id={listId}
          aria-label="Locations"
          className="absolute inset-x-0 top-[54px] z-20 max-h-72 overflow-auto overscroll-contain rounded-xl border border-line bg-surface p-1.5 text-fg shadow-(--modal-shadow)"
        >
          {ordered.map((p) => (
            <li key={p.id}>
              <button
                type="button"
                aria-current={p.id === selected?.id ? "true" : undefined}
                onClick={() => {
                  onPick(p.id);
                  setOpen(false);
                }}
                className={`flex h-[42px] w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-[13px] font-semibold hover:bg-raised ${
                  p.id === selected?.id ? "bg-raised" : ""
                }`}
              >
                <CountryFlag code={p.server.location?.country_code} />
                <span className="min-w-0 flex-1 truncate">
                  {locationLabel(p, countries)}
                  <span className="ml-2 text-[12px] font-medium text-muted">
                    {p.name}
                    {p.id === preferredId ? " (preferred)" : ""}
                  </span>
                </span>
                {p.server.favorite && (
                  <Star size={13} aria-label="Favorite" className="fill-current text-pending" />
                )}
                {p.id === selected?.id && <CheckIcon size={16} aria-hidden className="text-accent" />}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
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
        {country ? <span className="text-(--h-mute)"> · {country} (server's listed country)</span> : null}
      </span>
    );
  } else if (feature.status === "configured") {
    body = feature.detail;
  } else if (feature.status === "failed" || feature.status === "unsupported") {
    body = <span className="text-[#f0786a]">{feature.detail}</span>;
  } else if (connected) {
    body = "Enable connectivity check in Settings";
  } else {
    body = undefined;
  }
  return (
    <Detail label="DNS resolver egress IPv4" state={state}>
      {body}
    </Detail>
  );
}

const mbps = (n: number | null) => (n === null ? "Not measured" : `${n.toFixed(1)} Mbps`);
const ms = (n: number | null) => (n === null ? "Not measured" : `${Math.round(n)} ms`);

/** Shows the connection's speed test; it runs by itself when allowed, or when pressed. */
function SpeedPanel({ connected, speed, auto }: { connected: boolean; speed: SpeedRun; auto: boolean }) {
  const { busy, result, error, start } = speed;

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
          : auto
            ? "Starts by itself a few seconds after the connection is stable. Uses about 30 MB of data and contacts Cloudflare's speed server. Turn this off in Settings."
            : "Uses about 30 MB of data and contacts Cloudflare's speed server only when you press the button."}
      </p>
    </Panel>
  );
}

const LEAK_ROWS: {
  id: string;
  title: string;
  icon: ReactNode;
  protection?: "kill_switch" | "dns" | "ipv6";
  help?: HelpKey;
}[] = [
  {
    id: "kill_switch",
    title: "Kill switch",
    icon: <ShieldCheck size={20} aria-hidden />,
    protection: "kill_switch",
    help: "killSwitch",
  },
  {
    id: "dns",
    title: "DNS leak protection",
    icon: <Network size={20} aria-hidden />,
    protection: "dns",
    help: "dnsProtection",
  },
  {
    id: "ipv6",
    title: "IPv6 protection",
    icon: <Globe size={20} aria-hidden />,
    protection: "ipv6",
    help: "ipv6Protection",
  },
  { id: "reachability", title: "Server reachability", icon: <Radio size={20} aria-hidden /> },
  { id: "public_ip", title: "Public IP", icon: <MapPin size={20} aria-hidden /> },
  { id: "other_vpn", title: "Other VPN apps", icon: <Layers size={20} aria-hidden /> },
  { id: "browser_dns", title: "Browser DNS", icon: <Compass size={20} aria-hidden /> },
];
const LEAK_IDS = LEAK_ROWS.map((r) => r.id);
const leakTone: Record<CheckStatus, Tone> = { pass: "ok", warn: "pending", fail: "bad", skipped: "none" };
const leakText: Record<CheckStatus, string> = {
  pass: "Protected",
  warn: "Check this",
  fail: "Leaking",
  skipped: "Not tested",
};
const tagClass: Record<Tone, string> = {
  ok: "bg-ok-soft text-ok",
  pending: "bg-pending-soft text-pending",
  bad: "bg-danger-soft text-danger",
  none: "bg-raised text-muted",
};

/** Attached under the hero. Before a test runs, the three protections show their live status. */
function LeakSection({
  connected,
  protection,
  goTo,
}: {
  connected: boolean;
  protection?: { kill_switch: Feature; dns: Feature; ipv6: Feature };
  goTo: (s: ScreenId) => void;
}) {
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

  // Check automatically a few seconds after connecting; "Test again" reruns by hand.
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
    <div className="leak-section">
      <div className="flex flex-wrap items-center justify-between gap-3 px-6 pt-5 pb-1.5">
        <h2 className="text-[16px] font-bold">
          Leak test
          <InfoTip topic="leakTest" />
        </h2>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => goTo("settings")}
            className="text-[12.5px] font-bold text-accent hover:underline"
          >
            Manage in Settings
          </button>
          <Button
            variant="primary"
            className="min-w-28 whitespace-nowrap"
            onClick={start}
            disabled={!connected || busy}
          >
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
        </div>
      </div>
      <p className="px-6 pb-2 text-[13px] text-muted">
        {!connected
          ? "Connect to a server to run a leak test."
          : result
            ? worst
            : "Checks run by themselves a few seconds after you connect."}
      </p>
      <ul aria-label="Leak checks">
        {LEAK_ROWS.map((row) => {
          const c = result?.checks.find((x) => x.id === row.id);
          const f = row.protection ? protection?.[row.protection] : undefined;
          let tone: Tone = "none";
          let tag = "Not run";
          let detail = "";
          if (c) {
            tone = leakTone[c.status];
            tag = leakText[c.status];
            detail = c.detail;
          } else if (f) {
            tone = featureTone(f);
            tag = featureText(f);
            detail = "detail" in f ? f.detail : "";
          }
          return (
            <li
              key={row.id}
              className="grid grid-cols-[44px_minmax(0,1fr)_auto] items-center gap-4 border-t border-line px-6 py-3.5"
            >
              <span className="grid size-11 place-items-center rounded-[10px] bg-raised text-muted">
                {row.icon}
              </span>
              <div className="min-w-0">
                <p className="text-[14px] font-bold">
                  {c?.title ?? row.title}
                  {row.help && <InfoTip topic={row.help} />}
                </p>
                {detail && <p className="text-[12.5px] break-words text-muted">{detail}</p>}
                {row.id === "other_vpn" && c?.status === "warn" && (
                  <div className="mt-1.5">
                    <Button onClick={stopOthers} disabled={stopping || busy}>
                      {stopping ? "Stopping…" : "Stop it"}
                    </Button>
                    {stopNote && <p className="mt-1 text-[12px] text-muted">{stopNote}</p>}
                  </div>
                )}
              </div>
              <span className={`rounded-full px-2.5 py-0.5 text-[11.5px] font-bold ${tagClass[tone]}`}>
                {tag}
              </span>
            </li>
          );
        })}
      </ul>
      {error && (
        <p role="alert" className="px-6 py-2 text-[13px] text-danger">
          {error}
        </p>
      )}
      <p className="border-t border-line px-6 py-3 text-[12px] text-muted">
        {result
          ? `Checked at ${clock(result.at)}. Tests run on this device and contact no third-party site.`
          : "Runs on this device and contacts no third-party site."}
      </p>
    </div>
  );
}
