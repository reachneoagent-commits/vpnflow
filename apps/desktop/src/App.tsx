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
          className="flex w-[13.5rem] shrink-0 flex-col border-r border-line/70 max-[720px]:w-16"
        >
          <div className="flex items-center gap-3 px-5 pt-6 pb-7 max-[720px]:justify-center max-[720px]:px-0">
            <Shield confirmed={tone === "ok"} />
            <div className="min-w-0 max-[720px]:hidden">
              <p className="text-[19px] leading-none font-bold tracking-[-0.02em]">VPNFlow</p>
              <p className="mt-0.5 text-[11px] tracking-[0.04em] text-muted uppercase">VPN</p>
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
                        ? "bg-surface font-semibold text-fg ring-1 ring-line"
                        : subordinate
                          ? "text-muted/90 hover:bg-surface/50 hover:text-fg"
                          : "text-muted hover:bg-surface/60 hover:text-fg"
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
    <div
      aria-live="polite"
      className="pointer-events-none fixed right-4 bottom-4 flex w-[min(420px,calc(100%-32px))] flex-col gap-2"
    >
      {notices.map((n) => (
        <div
          key={n.id}
          role={n.tone === "error" ? "alert" : "status"}
          className={`pointer-events-auto flex items-start gap-3 rounded-xl border px-3.5 py-2.5 text-[13px] shadow-[0_12px_32px_-12px_rgb(10_16_22/0.35)] ${
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

/** Brand mark doubles as status: the check appears only once the server has answered. */
function Shield({ confirmed }: { confirmed: boolean }) {
  return (
    <svg
      width="28"
      height="28"
      viewBox="0 0 24 24"
      aria-hidden
      className={`shrink-0 transition-colors duration-[420ms] ${confirmed ? "text-ok" : "text-fg"}`}
    >
      <path
        d="M12 2.5 4 5.5v6.2c0 4.9 3.4 8.6 8 9.8 4.6-1.2 8-4.9 8-9.8V5.5l-8-3Z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
      {confirmed && (
        <path
          d="M8.5 12.2 11 14.6l4.6-5"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
        />
      )}
    </svg>
  );
}
