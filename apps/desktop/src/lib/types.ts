// Mirrors crates/aegis-core/src/types.rs. No type here may carry key material.

export type ProfileId = string;
export type RouteKind = "full" | "full_ipv4_only" | "full_ipv6_only" | "split";
export type BackendKind = "mock" | "network_manager" | "windows_service" | "macos_daemon" | "none";
export type Support = { status: "supported" } | { status: "unsupported"; detail: string };
export type PeerState = "pending" | "confirmed" | "no_response";
export type Level = "info" | "warn" | "error";
export type Theme = "system" | "light" | "dark";

export interface ProfileSummary {
  interface_public_key: string;
  addresses: string[];
  dns: string[];
  endpoints: string[];
  allowed_ips: string[];
  route_kind: RouteKind;
  peer_count: number;
  mtu: number | null;
  has_preshared_key: boolean;
  warnings: string[];
}

export interface ProfileView {
  id: ProfileId;
  name: string;
  created_ms: number;
  updated_ms: number;
  summary: ProfileSummary;
  preferred: boolean;
  active: boolean;
  server: ServerMetadata;
}

export interface Country {
  code: string;
  name: string;
}
export interface VpnLocation {
  country_code: string;
  city: string | null;
  region: string | null;
  time_zone: string | null;
}
export interface ServerUpdate {
  location: VpnLocation | null;
  favorite: boolean;
  enabled: boolean;
  registration_pending: boolean;
}
export interface ServerMetadata extends ServerUpdate {
  last_connected_ms: number | null;
  last_attempt_ms: number | null;
  last_failure_ms: number | null;
  last_response_ms: number | null;
}
export interface ServerHealth {
  id: ProfileId;
  reachability: "unknown" | "responded" | "no_response";
  observed_ms: number | null;
  measured_rtt_ms: number | null;
  last_handshake_ms: number | null;
}
export interface QuickConnectChoice {
  id: ProfileId;
  reason: string;
}

export interface Capabilities {
  backend: BackendKind;
  simulated: boolean;
  backend_version: string | null;
  available: Support;
  kill_switch: Support;
  dns_protection: Support;
  ipv6_protection: Support;
  exact_handshake_time: Support;
  notes: string[];
}

export type ConnectionState =
  | { state: "disconnected" }
  | { state: "preparing"; profile: ProfileId }
  | { state: "connecting"; profile: ProfileId }
  | { state: "connected"; profile: ProfileId; since_ms: number; peer: PeerState }
  | { state: "reconnecting"; profile: ProfileId; attempt: number; next_retry_ms: number }
  | { state: "disconnecting"; profile: ProfileId }
  | { state: "uncertain"; profile: ProfileId | null; reason: string }
  | { state: "failed"; profile: ProfileId | null; reason: string };

export type Feature =
  | { status: "off" }
  | { status: "unsupported"; detail: string }
  | { status: "configured"; detail: string }
  | { status: "verified"; detail: string }
  | { status: "failed"; detail: string };

export interface Protection {
  kill_switch: Feature;
  dns: Feature;
  ipv6: Feature;
}

export interface Stats {
  rx_bytes: number;
  tx_bytes: number;
  rx_rate_bps: number;
  tx_rate_bps: number;
  last_handshake_ms: number | null;
  last_response_ms: number | null;
  interface: string | null;
  sampled_ms: number;
}

export interface Snapshot {
  health: ServerHealth[];
  state: ConnectionState;
  protection: Protection;
  /** Whether the tunnel's DNS server answered a lookup. "off" = not tested. */
  reachability: Feature;
  /** Egress IPv4 from tunnel DNS whoami. Verified detail is the address. */
  public_ip: Feature;
  stats: Stats | null;
  profile: ProfileView | null;
  simulated: boolean;
}

export interface LogEntry {
  at_ms: number;
  level: Level;
  message: string;
}

export interface Settings {
  theme: Theme;
  close_to_tray: boolean;
  launch_on_login: boolean;
  connect_on_launch: boolean;
  preferred_profile: ProfileId | null;
  favorite_countries: string[];
  quick_connect_country: string | null;
  auto_reconnect: boolean;
  reconnect_max_attempts: number;
  kill_switch: boolean;
  kill_switch_allow_lan: boolean;
  notifications: boolean;
  connectivity_check: boolean;
  check_updates: boolean;
}

export type KeyAction = "keep" | "generate" | "remove";

export interface PeerDraft {
  public_key: string;
  preshared_key: KeyAction;
  has_preshared_key: boolean;
  endpoint: string;
  allowed_ips: string[];
  persistent_keepalive: number | null;
}

export interface ProfileDraft {
  name: string;
  private_key: KeyAction;
  public_key: string | null;
  addresses: string[];
  dns: string[];
  mtu: number | null;
  listen_port: number | null;
  peers: PeerDraft[];
}

export interface Issue {
  severity: "error" | "warning";
  line: number | null;
  field: string | null;
  message: string;
}

export interface UiError {
  kind: string;
  message: string;
  issues: Issue[];
}

export type CheckStatus = "pass" | "warn" | "fail" | "skipped";
export interface Check {
  id: string;
  title: string;
  status: CheckStatus;
  detail: string;
}

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
  tray_available: boolean;
}

export type EngineEvent =
  | { type: "snapshot"; data: Snapshot }
  | { type: "log"; data: LogEntry }
  | { type: "profiles_changed" }
  | { type: "settings_changed"; data: Settings };
