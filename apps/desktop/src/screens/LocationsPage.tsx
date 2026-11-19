import { useState } from "react";
import { Locations } from "./Locations";
import { Profiles } from "./Profiles";

type Tab = "locations" | "advanced";

const TABS: { id: Tab; label: string }[] = [
  { id: "locations", label: "Locations" },
  { id: "advanced", label: "Advanced WireGuard" },
];

/** One page for picking servers and for editing the WireGuard profiles behind them. */
export function LocationsPage({ initialTab = "locations" }: { initialTab?: Tab }) {
  const [tab, setTab] = useState<Tab>(initialTab);
  return (
    <div className="flex flex-col gap-5">
      <div role="tablist" aria-label="Locations sections" className="flex gap-1 border-b border-line">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            id={`tab-${t.id}`}
            aria-selected={tab === t.id}
            aria-controls={`panel-${t.id}`}
            onClick={() => setTab(t.id)}
            className={`-mb-px h-11 border-b-2 px-4 font-semibold ${
              tab === t.id ? "border-accent text-fg" : "border-transparent text-muted hover:text-fg"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
        {tab === "locations" ? <Locations /> : <Profiles />}
      </div>
    </div>
  );
}
