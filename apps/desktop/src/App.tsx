import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  Globe,
  Info,
  LayoutDashboard,
  Moon,
  Settings as SettingsIcon,
  Sun,
  UserRound,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { HelperBanner } from "./components/HelperBanner";
import { Shield } from "./components/Shield";
import { Dot } from "./components/ui";
import { stateHeadline, stateTone } from "./lib/connection";
import { StoreContext, useAppStore, useStore } from "./lib/store";
import { About } from "./screens/About";
import { ActivityScreen } from "./screens/Activity";
import { Dashboard } from "./screens/Dashboard";
import { LocationsPage } from "./screens/LocationsPage";
import { Onboarding } from "./screens/Onboarding";
import { SettingsScreen } from "./screens/Settings";

const screens = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "locations", label: "Locations", icon: Globe },
  { id: "activity", label: "Activity", icon: Activity },
  { id: "settings", label: "Settings", icon: SettingsIcon },
  { id: "about", label: "About", icon: Info },
] as const;
/** "profiles" (Advanced WireGuard) is a tab inside Locations, not a separate menu entry. */
export type ScreenId = (typeof screens)[number]["id"] | "profiles";

export function App() {
  const store = useAppStore();
  return (
    <StoreContext.Provider value={store}>
      <Shell />
    </StoreContext.Provider>
  );
}

function Shell() {
  const store = useStore();
  const [screen, setScreen] = useState<ScreenId>("dashboard");
  const [onboardingDone, setOnboardingDone] = useState(false);
  const [setupOpen, setSetupOpen] = useState(false);
  useEffect(() => {
    const open = () => setSetupOpen(true);
    window.addEventListener("vpnflow:setup", open);
    // Dev builds only: Cmd/Ctrl+Shift+O reopens setup so it can be previewed.
    const onKey = (e: KeyboardEvent) => {
      if (import.meta.env.DEV && (e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "o")
        open();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("vpnflow:setup", open);
      window.removeEventListener("keydown", onKey);
    };
  }, []);
  useEffect(() => {
    const p = listen<ScreenId>("navigate", (e) => setScreen(e.payload));
    return () => void p.then((u) => u());
  }, []);

  if (!store.ready) {
    return (
      <div className="grid h-full place-items-center text-muted" role="status">
        Starting…
        <Notices />
      </div>
    );
  }
  // First run only: nothing set up yet and setup not finished or skipped.
  if (setupOpen || (!store.settings?.onboarded && !onboardingDone && store.profiles.length === 0)) {
    return (
      <div className="h-full overflow-y-auto">
        <Onboarding
          onDone={() => {
            setOnboardingDone(true);
            setSetupOpen(false);
          }}
        />
        <Notices />
      </div>
    );
  }
  const tone = stateTone(store.snapshot?.state);
  const active = (id: ScreenId) => screen === id || (id === "locations" && screen === "profiles");

  return (
    <div className="flex h-full flex-col">
      {store.caps?.simulated && (
        <div
          role="alert"
          className="border-b border-pending/40 bg-pending-soft px-4 py-1.5 text-[13px] text-pending"
        >
          Simulated backend. Profiles are temporary. No real tunnel is created and your traffic is not
          protected.
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <nav
          aria-label="Main"
          className="flex w-[216px] shrink-0 flex-col border-r border-white/5 bg-ink text-mist max-[720px]:w-16"
        >
          <div className="flex items-center justify-center gap-3 px-3 pt-6 pb-7">
            <Shield confirmed={tone === "ok"} />
            <div className="min-w-0 max-[720px]:hidden">
              <p className="text-[19px] leading-none font-bold tracking-[-0.02em]">VPNFlow</p>
            </div>
          </div>
          <ul className="flex flex-col gap-0.5 px-3">
            {screens.map((s) => {
              return (
                <li key={s.id}>
                  <button
                    type="button"
                    onClick={() => setScreen(s.id)}
                    aria-current={active(s.id) ? "page" : undefined}
                    title={s.label}
                    className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-[14px] max-[720px]:justify-center max-[720px]:px-0 ${
                      active(s.id)
                        ? "bg-brand/15 font-semibold text-brand"
                        : "text-mist/70 hover:bg-white/5 hover:text-mist"
                    }`}
                  >
                    <s.icon size={17} strokeWidth={active(s.id) ? 2.2 : 1.8} aria-hidden />
                    <span className="max-[720px]:sr-only">{s.label}</span>
                    {s.id === "dashboard" && (
                      <span className="ml-auto [--ok:#5bc893] [--pending:#e3a84b] max-[720px]:hidden">
                        <Dot tone={tone} />
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        </nav>
        <div className="flex min-w-0 flex-1 flex-col">
          <TopBar />
          <main className="min-w-0 flex-1 overflow-y-auto">
            <div className="mx-auto max-w-[940px] px-8 pt-7 pb-12 max-[720px]:px-4 max-[720px]:pt-5">
              <HelperBanner />
              {screen === "dashboard" && <Dashboard goTo={setScreen} />}
              {(screen === "locations" || screen === "profiles") && (
                <LocationsPage initialTab={screen === "profiles" ? "advanced" : "locations"} />
              )}
              {screen === "activity" && <ActivityScreen />}
              {screen === "settings" && <SettingsScreen />}
              {screen === "about" && <About />}
            </div>
          </main>
        </div>
      </div>
      <Notices />
    </div>
  );
}

function TopBar() {
  const { snapshot, settings, saveSettings } = useStore();
  const tone = stateTone(snapshot?.state);
  const headline = snapshot ? stateHeadline(snapshot.state) : "Starting";
  const systemDark =
    typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches === true;
  const dark = settings?.theme === "dark" || (settings?.theme !== "light" && systemDark);
  return (
    <header className="flex h-[60px] shrink-0 items-center gap-3 border-b border-line bg-surface px-6">
      <span
        className="inline-flex items-center gap-2 rounded-full bg-raised px-3 py-1 text-[12.5px] font-semibold"
        role="status"
      >
        <Dot tone={tone} />
        {headline}
      </span>
      <div className="ml-auto flex items-center gap-3">
        <button
          type="button"
          onClick={() => settings && void saveSettings({ ...settings, theme: dark ? "light" : "dark" })}
          aria-label={dark ? "Switch to light theme" : "Switch to dark theme"}
          title={dark ? "Switch to light theme" : "Switch to dark theme"}
          className="grid size-9 place-items-center rounded-full bg-raised text-muted hover:text-fg"
        >
          {dark ? <Sun size={17} aria-hidden /> : <Moon size={17} aria-hidden />}
        </button>
        <div className="flex items-center gap-2.5">
          <span className="grid size-9 place-items-center rounded-full bg-raised text-muted" aria-hidden>
            <UserRound size={17} />
          </span>
          <div className="leading-tight max-[720px]:hidden">
            <p className="text-[13px] font-semibold">This device</p>
            <p className="text-[11.5px] text-muted">Local only · no account</p>
          </div>
        </div>
      </div>
    </header>
  );
}

function Notices() {
  const { notices, dismiss } = useStore();
  return (
    <div aria-live="polite" className="pointer-events-none fixed inset-x-0 top-0 z-50 flex flex-col gap-px">
      {notices.map((n) => (
        <div
          key={n.id}
          role={n.tone === "error" ? "alert" : "status"}
          className={`pointer-events-auto flex items-start gap-3 border-b px-5 py-2.5 text-[13px] shadow-[0_8px_24px_-12px_rgb(10_16_22/0.35)] ${
            n.tone === "error" ? "border-danger/50 bg-danger-soft text-fg" : "border-line bg-surface text-fg"
          }`}
        >
          <div className="min-w-0 flex-1">
            <p>{n.text}</p>
            {n.error && n.error.issues.length > 1 && (
              <ul className="mt-1 list-disc pl-4 text-[12px] text-muted">
                {n.error.issues.slice(0, 6).map((i, k) => (
                  <li key={`${k}-${i.message}`}>
                    {i.line ? `Line ${i.line}: ` : ""}
                    {i.field ? `${i.field} ` : ""}
                    {i.message}
                  </li>
                ))}
              </ul>
            )}
          </div>
          <button
            type="button"
            onClick={() => dismiss(n.id)}
            aria-label="Dismiss"
            className="text-muted hover:text-fg"
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
