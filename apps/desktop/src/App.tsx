import { listen } from "@tauri-apps/api/event";
import { Activity, Globe, Info, KeyRound, LayoutDashboard, Settings as SettingsIcon, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Dot } from "./components/ui";
import { stateTone } from "./lib/connection";
import { StoreContext, useAppStore, useStore } from "./lib/store";
import { About } from "./screens/About";
import { ActivityScreen } from "./screens/Activity";
import { Dashboard } from "./screens/Dashboard";
import { Locations } from "./screens/Locations";
import { Onboarding } from "./screens/Onboarding";
import { Profiles } from "./screens/Profiles";
import { SettingsScreen } from "./screens/Settings";

const screens = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "locations", label: "Locations", icon: Globe },
  { id: "profiles", label: "Advanced WireGuard", icon: KeyRound, subordinate: true },
  { id: "activity", label: "Activity", icon: Activity },
  { id: "settings", label: "Settings", icon: SettingsIcon },
  { id: "about", label: "About", icon: Info },
] as const;
export type ScreenId = (typeof screens)[number]["id"];

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
    return () => window.removeEventListener("vpnflow:setup", open);
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
          className="flex w-[13.5rem] shrink-0 flex-col border-r border-white/5 bg-ink text-mist max-[720px]:w-16"
        >
          <div className="flex items-center gap-3 px-5 pt-6 pb-7 max-[720px]:justify-center max-[720px]:px-0">
            <Shield confirmed={tone === "ok"} />
            <div className="min-w-0 max-[720px]:hidden">
              <p className="text-[19px] leading-none font-bold tracking-[-0.02em]">VPNFlow</p>
              <p className="mt-0.5 text-[11px] tracking-[0.04em] text-mist/60 uppercase">VPN</p>
            </div>
          </div>
          <ul className="flex flex-col gap-0.5 px-3">
            {screens.map((s) => {
              const subordinate = "subordinate" in s && s.subordinate;
              return (
                <li key={s.id} className={subordinate ? "mt-1" : undefined}>
                  <button
                    type="button"
                    onClick={() => setScreen(s.id)}
                    aria-current={screen === s.id ? "page" : undefined}
                    title={s.label}
                    className={`flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left max-[720px]:justify-center max-[720px]:px-0 ${
                      subordinate ? "text-[12.5px]" : "text-[14px]"
                    } ${
                      screen === s.id
                        ? "bg-brand/15 font-semibold text-brand"
                        : subordinate
                          ? "text-mist/60 hover:bg-white/5 hover:text-mist"
                          : "text-mist/70 hover:bg-white/5 hover:text-mist"
                    }`}
                  >
                    <s.icon
                      size={subordinate ? 15 : 17}
                      strokeWidth={screen === s.id ? 2.2 : 1.8}
                      aria-hidden
                    />
                    <span className="max-[720px]:sr-only">{s.label}</span>
                    {s.id === "dashboard" && (
                      <span className="ml-auto max-[720px]:hidden">
                        <Dot tone={tone} />
                      </span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        </nav>
        <main className="min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[920px] px-8 pt-8 pb-12 max-[720px]:px-4 max-[720px]:pt-5">
            {screen === "dashboard" && <Dashboard goTo={setScreen} />}
            {screen === "locations" && <Locations />}
            {screen === "profiles" && <Profiles />}
            {screen === "activity" && <ActivityScreen />}
            {screen === "settings" && <SettingsScreen />}
            {screen === "about" && <About />}
          </div>
        </main>
      </div>
      <Notices />
    </div>
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

/** Brand mark doubles as status: it turns green only once the server has answered. */
function Shield({ confirmed }: { confirmed: boolean }) {
  return (
    <svg
      width="34"
      height="28"
      viewBox="0 0 44.89 37.23"
      aria-hidden
      className={`shrink-0 transition-colors duration-[420ms] ${confirmed ? "text-ok" : "text-brand"}`}
    >
      <polygon fill="currentColor" points="0 17.22 8.65 18.04 8.65 16.39 0 17.22" />
      <path
        fill="currentColor"
        d="M44.89,18.08l-7.21-.69h0c.53-4.21.71-8.21.76-9.56.01-.28-.18-.54-.45-.61C30.13,4.99,22.29,0,22.29,0l.02.11s0,0,0,0l-.02,2.45c-2.06,1.7-4.9,3-7.39,3.87-1.25.44-2.55.83-3.84,1.18l-1.94.51c-.27.07-.47.33-.47.62v7.65s1.1-.1,1.1-.1c6.31-.6,11.86-5.6,12.46-11.9l.08-.85-.19,30.72-.45-4.2c-.6-6.31-5.6-11.3-11.9-11.9l-1.09-.1v.69c0,5.16,2.57,9.96,6.86,12.82,0,0,6.19,4.08,6.57,4.41.02.01-.03,1.26-.03,1.26.17-.05,9.91-4.57,12.76-8.96,1.29-1.99,2.13-5.72,2.68-9.48,0,0,0,0,0,0l7.39-.71ZM36.03,9.72c-.12,2.27-.33,4.9-.66,7.44,0,0,0,0,0,0h-.02s-.21-.02-.21-.02c-6.31-.6-11.3-5.6-11.9-11.9l-.16-1.69v-.04c2.48,1.47,7.06,3.87,12.52,5.59.27.08.44.33.43.61ZM32.85,27.19c-1.6,2.45-7.72,5.82-9.92,6.97l.31-3.24c.61-6.29,5.57-11.28,11.86-11.9,0,0,.02,0,.02,0h0c-.53,3.52-1.28,6.66-2.27,8.18Z"
      />
    </svg>
  );
}
