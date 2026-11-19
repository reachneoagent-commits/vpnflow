// Thin typed wrapper over Tauri commands. File paths are never passed from
// here: import/export open native dialogs on the Rust side.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppInfo,
  Capabilities,
  Check,
  Country,
  EngineEvent,
  LogEntry,
  ProfileDraft,
  ProfileId,
  ProfileView,
  QuickConnectChoice,
  ServerHealth,
  ServerUpdate,
  Settings,
  Snapshot,
  SpeedResult,
  UiError,
} from "./types";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  capabilities: () => invoke<Capabilities>("capabilities"),
  snapshot: () => invoke<Snapshot>("snapshot"),
  events: () => invoke<LogEntry[]>("events"),
  profiles: () => invoke<ProfileView[]>("profiles"),
  countries: () => invoke<Country[]>("country_catalog"),
  updateServer: (id: ProfileId, update: ServerUpdate) => invoke<ProfileView>("update_server", { id, update }),
  serverHealth: () => invoke<ServerHealth[]>("server_health"),
  refreshServerHealth: () => invoke<ServerHealth[]>("refresh_server_health"),
  quickConnect: () => invoke<QuickConnectChoice>("quick_connect"),
  settings: () => invoke<Settings>("settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  connect: (id: ProfileId) => invoke<void>("connect", { id }),
  disconnect: () => invoke<void>("disconnect"),
  cancel: () => invoke<void>("cancel"),
  importDialog: () => invoke<ProfileView[]>("import_dialog"),
  newDraft: () => invoke<ProfileDraft>("new_draft"),
  draft: (id: ProfileId) => invoke<ProfileDraft>("profile_draft", { id }),
  createProfile: (draft: ProfileDraft) => invoke<ProfileView>("create_profile", { draft }),
  updateProfile: (id: ProfileId, draft: ProfileDraft) => invoke<ProfileView>("update_profile", { id, draft }),
  duplicateProfile: (id: ProfileId) => invoke<ProfileView>("duplicate_profile", { id }),
  deleteProfile: (id: ProfileId) => invoke<void>("delete_profile", { id }),
  exportProfile: (id: ProfileId) => invoke<boolean>("export_profile", { id }),
  diagnostics: () => invoke<Check[]>("diagnostics"),
  exportDiagnostics: () => invoke<boolean>("export_diagnostics"),
  openLink: (which: string) => invoke<void>("open_link", { which }),
  scanDownloads: (sinceMs: number) =>
    invoke<{ imported: string[]; failed: number }>("scan_downloads", { sinceMs }),
  currentNetwork: () => invoke<string | null>("current_network"),
  speedTest: () => invoke<SpeedResult>("speed_test"),
  stopOtherVpns: () => invoke<{ name: string; ok: boolean; message: string }[]>("stop_other_vpns"),
};

export function onEngineEvent(cb: (e: EngineEvent) => void): Promise<UnlistenFn> {
  return listen<EngineEvent>("engine", (e) => cb(e.payload));
}

export function onImportResult(cb: (r: { name: string; error: UiError | null }) => void) {
  return listen<{ name: string; error: UiError | null }>("import-result", (e) => cb(e.payload));
}

export function asUiError(e: unknown): UiError {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as Partial<UiError>;
    return { kind: o.kind ?? "unknown", message: String(o.message), issues: o.issues ?? [] };
  }
  return { kind: "unknown", message: String(e), issues: [] };
}
