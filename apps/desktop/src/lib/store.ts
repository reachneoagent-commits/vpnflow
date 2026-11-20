import { listen } from "@tauri-apps/api/event";
import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { api, asUiError, onEngineEvent, onImportResult } from "./api";
import type {
  AppInfo,
  Capabilities,
  Country,
  LogEntry,
  ProfileView,
  Settings,
  Snapshot,
  UiError,
} from "./types";

export interface Notice {
  id: number;
  tone: "info" | "error";
  text: string;
  error?: UiError;
}

export interface AppStore {
  ready: boolean;
  info: AppInfo | null;
  caps: Capabilities | null;
  snapshot: Snapshot | null;
  profiles: ProfileView[];
  countries: Country[];
  settings: Settings | null;
  /** Set while the VPN is paused: when it will turn back on. */
  pause: { until_ms: number } | null;
  events: LogEntry[];
  notices: Notice[];
  notify: (n: Omit<Notice, "id">) => void;
  dismiss: (id: number) => void;
  /** Run an action, turning failures into an error notice. Returns success. */
  run: (f: () => Promise<unknown>, okText?: string) => Promise<boolean>;
  refreshProfiles: () => Promise<void>;
  saveSettings: (s: Settings) => Promise<void>;
}

export const StoreContext = createContext<AppStore | null>(null);

export function useStore(): AppStore {
  const s = useContext(StoreContext);
  if (!s) throw new Error("StoreContext missing");
  return s;
}

let nextId = 1;
const MAX_EVENTS = 200;

export function useAppStore(): AppStore {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [caps, setCaps] = useState<Capabilities | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [countries, setCountries] = useState<Country[]>([]);
  const [profiles, setProfiles] = useState<ProfileView[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [pause, setPause] = useState<{ until_ms: number } | null>(null);
  const [events, setEvents] = useState<LogEntry[]>([]);
  const [notices, setNotices] = useState<Notice[]>([]);

  const notify = useCallback((n: Omit<Notice, "id">) => {
    const id = nextId++;
    setNotices((xs) => [...xs.slice(-3), { ...n, id }]);
    if (n.tone === "info") setTimeout(() => setNotices((xs) => xs.filter((x) => x.id !== id)), 4000);
  }, []);
  const dismiss = useCallback((id: number) => setNotices((xs) => xs.filter((x) => x.id !== id)), []);

  const refreshProfiles = useCallback(async () => setProfiles(await api.profiles()), []);

  const run = useCallback(
    async (f: () => Promise<unknown>, okText?: string) => {
      try {
        await f();
        if (okText) notify({ tone: "info", text: okText });
        return true;
      } catch (e) {
        const error = asUiError(e);
        notify({ tone: "error", text: error.message, error });
        return false;
      }
    },
    [notify],
  );

  const saveSettings = useCallback(
    async (s: Settings) => {
      await run(async () => setSettings(await api.setSettings(s)));
    },
    [run],
  );

  useEffect(() => {
    let alive = true;
    const unlisten: Array<() => void> = [];
    (async () => {
      let sawSnapshot = false;
      let sawProfiles = false;
      let sawSettings = false;
      const listeners = await Promise.all([
        listen<{ until_ms: number } | null>("pause", (e) => alive && setPause(e.payload)),
        listen<string>("settings-error", (e) => notify({ tone: "error", text: e.payload })),
        onEngineEvent((e) => {
          if (!alive) return;
          if (e.type === "snapshot") {
            sawSnapshot = true;
            setSnapshot(e.data);
          } else if (e.type === "log") setEvents((xs) => [...xs.slice(-(MAX_EVENTS - 1)), e.data]);
          else if (e.type === "settings_changed") {
            sawSettings = true;
            setSettings(e.data);
          } else if (e.type === "profiles_changed") {
            sawProfiles = true;
            void api
              .profiles()
              .then((p) => {
                if (alive) setProfiles(p);
              })
              .catch((e) => notify({ tone: "error", text: asUiError(e).message }));
          }
        }),
        onImportResult((r) => {
          if (!alive) return;
          if (r.error) notify({ tone: "error", text: `${r.name}: ${r.error.message}`, error: r.error });
          else notify({ tone: "info", text: `Imported ${r.name}; assign a country in Locations` });
        }),
      ]);
      if (!alive) {
        for (const u of listeners) u();
        return;
      }
      unlisten.push(...listeners);
      const [i, c, s, p, st, ev, countries] = await Promise.all([
        api.appInfo(),
        api.capabilities(),
        api.snapshot(),
        api.profiles(),
        api.settings(),
        api.events(),
        api.countries(),
      ]);
      if (!alive) return;
      setPause(await api.pauseStatus());
      if (!alive) return;
      setCountries(countries);
      setInfo(i);
      setCaps(c);
      if (!sawSnapshot) setSnapshot(s);
      if (!sawProfiles) setProfiles(p);
      if (!sawSettings) setSettings(st);
      setEvents((xs) => [...ev, ...xs].slice(-MAX_EVENTS));
    })().catch((e) => notify({ tone: "error", text: asUiError(e).message }));
    return () => {
      alive = false;
      for (const u of unlisten) u();
    };
  }, [notify]);

  // Apply the theme to <html>; "system" follows prefers-color-scheme in CSS.
  useEffect(() => {
    if (!settings) return;
    const el = document.documentElement;
    if (settings.theme === "system") el.removeAttribute("data-theme");
    else el.setAttribute("data-theme", settings.theme);
  }, [settings]);

  return {
    ready: !!(info && caps && snapshot && settings),
    info,
    caps,
    snapshot,
    profiles,
    countries,
    settings,
    pause,
    events,
    notices,
    notify,
    dismiss,
    run,
    refreshProfiles,
    saveSettings,
  };
}
