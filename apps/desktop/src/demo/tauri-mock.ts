// Browser-only stand-in for the Tauri IPC layer, used by the website demo
// (vite.demo.config.ts aliases @tauri-apps/api/core and /event to this file).
// It plays a made-up WireGuard setup: demo servers on documentation addresses
// (RFC 5737), fake keys, and a simulated connection. Nothing touches a network.
import type {
  AppInfo,
  Capabilities,
  Check,
  ConnectionState,
  Country,
  EngineEvent,
  Feature,
  LogEntry,
  ProfileDraft,
  ProfileView,
  ServerHealth,
  ServerUpdate,
  Settings,
  Snapshot,
  Stats,
} from "../lib/types";

export type UnlistenFn = () => void;
type Handler = (event: { payload: unknown }) => void;

const listeners = new Map<string, Set<Handler>>();
export function listen<T>(event: string, cb: (e: { payload: T }) => void): Promise<UnlistenFn> {
  const set = listeners.get(event) ?? new Set<Handler>();
  set.add(cb as Handler);
  listeners.set(event, set);
  return Promise.resolve(() => void set.delete(cb as Handler));
}
function emit(event: string, payload: unknown) {
  for (const h of listeners.get(event) ?? []) h({ payload });
}
const engine = (e: EngineEvent) => emit("engine", e);

const PAIRS: Array<[string, string]> = [
  ["AU", "Australia"],
  ["BR", "Brazil"],
  ["CA", "Canada"],
  ["CH", "Switzerland"],
  ["DE", "Germany"],
  ["ES", "Spain"],
  ["FR", "France"],
  ["GB", "United Kingdom"],
  ["IN", "India"],
  ["IT", "Italy"],
  ["JP", "Japan"],
  ["NL", "Netherlands"],
  ["NO", "Norway"],
  ["SE", "Sweden"],
  ["SG", "Singapore"],
  ["US", "United States"],
];
const COUNTRIES: Country[] = PAIRS.map(([code, name]) => ({ code, name }));

const now = () => Date.now();
const ep = (p: ProfileView): string => p.summary.endpoints[0] ?? "";
const fakeKey = (n: number) =>
  `ZGVtby1rZXktbm90LXJlYWwt${String(n).padStart(2, "0")}QUFBQUFBQUFBQUFBQUFBQQ=`.slice(0, 44);

function makeProfile(
  i: number,
  name: string,
  cc: string,
  city: string,
  host: number,
  extra: Partial<ServerUpdate> = {},
): ProfileView {
  return {
    id: `demo-${i}`,
    name,
    created_ms: now() - 86_400_000 * (30 - i),
    updated_ms: now() - 86_400_000 * i,
    summary: {
      interface_public_key: fakeKey(i),
      addresses: [`10.66.0.${i + 1}/32`],
      dns: ["10.66.0.1"],
      endpoints: [`203.0.113.${host}:51820`],
      allowed_ips: ["0.0.0.0/0", "::/0"],
      route_kind: "full",
      peer_count: 1,
      mtu: 1420,
      has_preshared_key: false,
      warnings: [],
    },
    preferred: i === 1,
    active: false,
    server: {
      location: { country_code: cc, city, region: null, time_zone: null },
      favorite: i <= 2,
      enabled: true,
      registration_pending: false,
      last_connected_ms: i === 1 ? now() - 3_600_000 : null,
      last_attempt_ms: i === 1 ? now() - 3_600_000 : null,
      last_failure_ms: null,
      last_response_ms: now() - 600_000,
      ...extra,
    },
  };
}

let profiles: ProfileView[] = [
  makeProfile(1, "Zurich 1", "CH", "Zurich", 10),
  makeProfile(2, "Frankfurt 1", "DE", "Frankfurt", 11),
  makeProfile(3, "Amsterdam 1", "NL", "Amsterdam", 12),
  makeProfile(4, "New York 1", "US", "New York", 13),
  makeProfile(5, "Tokyo 1", "JP", "Tokyo", 14),
  makeProfile(6, "Singapore 1", "SG", "Singapore", 15),
];

let settings: Settings = {
  theme: "system",
  close_to_tray: false,
  launch_on_login: false,
  connect_on_launch: false,
  preferred_profile: "demo-1",
  favorite_countries: ["CH", "DE"],
  quick_connect_country: null,
  auto_reconnect: true,
  auto_switch_server: true,
  auto_speed_test: true,
  reconnect_max_attempts: 5,
  kill_switch: true,
  kill_switch_chosen: true,
  kill_switch_allow_lan: true,
  kill_switch_allow_tailscale: true,
  bypass: [],
  bypass_sites: [],
  dns_servers: [],
  trusted_networks_enabled: false,
  trusted_networks: [],
  schedule_enabled: false,
  schedule_connect: "09:00",
  schedule_disconnect: "17:00",
  schedule_days: [1, 2, 3, 4, 5],
  notifications: false,
  connectivity_check: true,
  check_updates: false,
  onboarded: true,
};

const off: Feature = { status: "off" };
let state: ConnectionState = { state: "disconnected" };
let stats: Stats | null = null;
let publicIp: Feature = off;
let protection = { kill_switch: off, dns: off, ipv6: off } as Snapshot["protection"];
let reachability: Feature = off;
let rx = 0;
let tx = 0;
let events: LogEntry[] = [
  { at_ms: now(), level: "info", message: "Demo started. Nothing here touches a network." },
];
let timers: number[] = [];

const rtt = (i: number) => [24, 31, 28, 96, 212, 188][i] ?? 60;
function health(): ServerHealth[] {
  return profiles.map((p, i) => ({
    id: p.id,
    reachability: "responded",
    observed_ms: now() - 60_000,
    measured_rtt_ms: rtt(i) + Math.round(Math.random() * 6),
    last_handshake_ms:
      state.state === "connected" && "profile" in state && state.profile === p.id ? now() - 20_000 : null,
  }));
}
const activeProfile = () => profiles.find((p) => "profile" in state && state.profile === p.id) ?? null;
const snapshot = (): Snapshot => ({
  health: health(),
  state,
  protection,
  reachability,
  public_ip: publicIp,
  stats,
  profile: activeProfile(),
  simulated: true,
});
const push = () => engine({ type: "snapshot", data: snapshot() });
function log(level: LogEntry["level"], message: string) {
  const e = { at_ms: now(), level, message };
  events = [...events.slice(-199), e];
  engine({ type: "log", data: e });
}
const later = (ms: number, f: () => void) => timers.push(window.setTimeout(f, ms));
const clearTimers = () => {
  for (const t of timers) clearTimeout(t);
  timers = [];
};

function connect(id: string) {
  const p = profiles.find((x) => x.id === id);
  if (!p) throw { kind: "not_found", message: "That profile no longer exists.", issues: [] };
  clearTimers();
  profiles = profiles.map((x) => ({ ...x, active: x.id === id }));
  state = { state: "preparing", profile: id };
  log("info", `Preparing ${p.name}`);
  push();
  later(700, () => {
    state = { state: "connecting", profile: id };
    log("info", `Connecting to ${ep(p)}`);
    push();
  });
  later(1900, () => {
    const since = now();
    state = { state: "connected", profile: id, since_ms: since, peer: "confirmed" };
    rx = 0;
    tx = 0;
    stats = {
      rx_bytes: 0,
      tx_bytes: 0,
      rx_rate_bps: 0,
      tx_rate_bps: 0,
      last_handshake_ms: since,
      last_response_ms: since,
      interface: "aegis-demo",
      sampled_ms: since,
    };
    protection = {
      kill_switch: { status: "verified", detail: "Blocking all traffic outside the tunnel." },
      dns: { status: "verified", detail: "systemd-resolved sends all lookups to the tunnel's DNS server." },
      ipv6: { status: "verified", detail: "IPv6 has no route outside the tunnel." },
    };
    reachability = { status: "verified", detail: "Tunnel DNS answered." };
    publicIp = { status: "verified", detail: ep(p).split(":")[0] ?? "" };
    profiles = profiles.map((x) =>
      x.id === id ? { ...x, server: { ...x.server, last_connected_ms: since, last_attempt_ms: since } } : x,
    );
    log("info", `Connected to ${p.name}`);
    engine({ type: "profiles_changed" });
    push();
    timers.push(
      window.setInterval(() => {
        const down = 250_000 + Math.random() * 380_000;
        const up = 20_000 + Math.random() * 40_000;
        rx += down;
        tx += up;
        const t = now();
        stats = {
          rx_bytes: Math.round(rx),
          tx_bytes: Math.round(tx),
          rx_rate_bps: Math.round(down),
          tx_rate_bps: Math.round(up),
          last_handshake_ms: t - 20_000,
          last_response_ms: t,
          interface: "aegis-demo",
          sampled_ms: t,
        };
        push();
      }, 1000),
    );
  });
}
function reset(to: ConnectionState) {
  clearTimers();
  state = to;
  stats = null;
  publicIp = off;
  reachability = off;
  protection = { kill_switch: off, dns: off, ipv6: off };
  profiles = profiles.map((x) => ({ ...x, active: false }));
  push();
}
function disconnect() {
  const p = activeProfile();
  if (!p) return;
  clearTimers();
  state = { state: "disconnecting", profile: p.id };
  push();
  later(600, () => {
    log("info", "Disconnected");
    reset({ state: "disconnected" });
  });
}

const caps: Capabilities = {
  backend: "mock",
  simulated: true,
  backend_version: null,
  available: { status: "supported" },
  kill_switch: { status: "supported" },
  dns_protection: { status: "supported" },
  ipv6_protection: { status: "supported" },
  exact_handshake_time: { status: "supported" },
  notes: ["This is the website demo. Every value is made up."],
};
const info: AppInfo = { version: "0.1.0", os: "linux", arch: "x86_64", tray_available: false };

const blankDraft = (): ProfileDraft => ({
  name: "",
  private_key: "generate",
  public_key: null,
  addresses: ["10.66.0.2/32"],
  dns: ["10.66.0.1"],
  mtu: null,
  listen_port: null,
  peers: [
    {
      public_key: "",
      preshared_key: "keep",
      has_preshared_key: false,
      endpoint: "203.0.113.10:51820",
      allowed_ips: ["0.0.0.0/0", "::/0"],
      persistent_keepalive: 25,
    },
  ],
});
const draftOf = (p: ProfileView): ProfileDraft => ({
  name: p.name,
  private_key: "keep",
  public_key: p.summary.interface_public_key,
  addresses: p.summary.addresses,
  dns: p.summary.dns,
  mtu: p.summary.mtu,
  listen_port: null,
  peers: [
    {
      public_key: fakeKey(90),
      preshared_key: "keep",
      has_preshared_key: false,
      endpoint: ep(p),
      allowed_ips: p.summary.allowed_ips,
      persistent_keepalive: 25,
    },
  ],
});
const notInDemo = (what: string) => {
  throw {
    kind: "demo",
    message: `${what} is not available in the browser demo. The real app opens a native file dialog.`,
    issues: [],
  };
};

let seq = 100;
function ok<T>(v: T): Promise<T> {
  return Promise.resolve(JSON.parse(JSON.stringify(v ?? null)));
}

export function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return ok(run(cmd, args)) as Promise<T>;
  } catch (e) {
    return Promise.reject(e);
  }
}

function run(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "app_info":
      return info;
    case "capabilities":
      return caps;
    case "snapshot":
      return snapshot();
    case "events":
      return events;
    case "profiles":
      return profiles;
    case "country_catalog":
      return COUNTRIES;
    case "settings":
      return settings;
    case "set_settings":
      settings = a.settings as Settings;
      engine({ type: "settings_changed", data: settings });
      return settings;
    case "server_health":
    case "refresh_server_health":
      return health();
    case "quick_connect": {
      const pick = profiles.find((p) => p.id === settings.preferred_profile) ?? profiles[0];
      if (!pick) throw { kind: "not_found", message: "There are no profiles.", issues: [] };
      return { id: pick.id, reason: "Your preferred location" };
    }
    case "connect":
      connect(a.id as string);
      return null;
    case "disconnect":
      disconnect();
      return null;
    case "cancel":
      reset({ state: "disconnected" });
      return null;
    case "update_server": {
      const u = a.update as ServerUpdate;
      profiles = profiles.map((p) => (p.id === a.id ? { ...p, server: { ...p.server, ...u } } : p));
      engine({ type: "profiles_changed" });
      return profiles.find((p) => p.id === a.id);
    }
    case "new_draft":
      return blankDraft();
    case "profile_draft":
      return draftOf(profiles.find((p) => p.id === a.id) as ProfileView);
    case "create_profile": {
      const d = a.draft as ProfileDraft;
      const p = makeProfile(++seq, d.name || "New profile", "CH", "Zurich", 20, { favorite: false });
      p.id = `demo-${seq}`;
      p.preferred = false;
      p.server.location = null;
      profiles = [...profiles, p];
      engine({ type: "profiles_changed" });
      return p;
    }
    case "update_profile": {
      const d = a.draft as ProfileDraft;
      profiles = profiles.map((p) => (p.id === a.id ? { ...p, name: d.name, updated_ms: now() } : p));
      engine({ type: "profiles_changed" });
      return profiles.find((p) => p.id === a.id);
    }
    case "duplicate_profile": {
      const src = profiles.find((p) => p.id === a.id) as ProfileView;
      const p = { ...src, id: `demo-${++seq}`, name: `${src.name} copy`, preferred: false, active: false };
      profiles = [...profiles, p];
      engine({ type: "profiles_changed" });
      return p;
    }
    case "delete_profile":
      if ("profile" in state && state.profile === a.id) disconnect();
      profiles = profiles.filter((p) => p.id !== a.id);
      engine({ type: "profiles_changed" });
      return null;
    case "diagnostics":
      return [
        { id: "backend", title: "VPN backend", status: "pass", detail: "Demo backend is running." },
        {
          id: "wireguard",
          title: "WireGuard tools",
          status: "skipped",
          detail: "Not checked in the browser demo.",
        },
        { id: "dns", title: "DNS protection", status: "pass", detail: "Simulated." },
      ] satisfies Check[];
    case "import_dialog":
      return notInDemo("Importing a config");
    case "export_profile":
      return notInDemo("Exporting a profile");
    case "export_diagnostics":
      return notInDemo("Exporting diagnostics");
    default:
      throw { kind: "demo", message: `The demo does not implement "${cmd}".`, issues: [] };
  }
}
