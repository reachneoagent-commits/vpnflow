import { useCallback, useEffect, useState } from "react";
import { Button, Dot, InfoTip, Panel, Segmented, type Tone } from "../components/ui";
import { api } from "../lib/api";
import { clock } from "../lib/format";
import { useStore } from "../lib/store";
import type { Check, CheckStatus } from "../lib/types";

const tone: Record<CheckStatus, Tone> = { pass: "ok", warn: "pending", fail: "bad", skipped: "none" };
const statusText: Record<CheckStatus, string> = {
  pass: "Pass",
  warn: "Attention",
  fail: "Fail",
  skipped: "Skipped",
};

export function ActivityScreen() {
  const { events, run, snapshot } = useStore();
  const [checks, setChecks] = useState<Check[] | null>(null);
  const [filter, setFilter] = useState<"all" | "problems">("all");

  const refresh = useCallback(() => run(async () => setChecks(await api.diagnostics())), [run]);
  // Re-run when the connection state changes so results stay current.
  // biome-ignore lint/correctness/useExhaustiveDependencies: state change is the trigger
  useEffect(() => {
    void refresh();
  }, [refresh, snapshot?.state.state]);

  const shown = filter === "all" ? events : events.filter((e) => e.level !== "info");

  return (
    <div className="flex flex-col gap-7">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-[28px] leading-tight font-semibold tracking-[-0.015em]">
          Activity and diagnostics
        </h1>
        <div className="flex gap-2">
          <Button onClick={refresh}>Run checks</Button>
          <Button onClick={() => run(api.exportDiagnostics)}>Export diagnostics</Button>
          <InfoTip topic="exportDiagnostics" />
        </div>
      </header>

      <Panel title="Checks" help="checks">
        {!checks ? (
          <p className="py-3 text-muted">Running…</p>
        ) : (
          <ul>
            {checks.map((c) => (
              <li key={c.id} className="flex items-start gap-3 border-b border-line py-2 last:border-b-0">
                <span className="pt-1.5">
                  <Dot tone={tone[c.status]} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex justify-between gap-3">
                    <span className="font-medium">{c.title}</span>
                    <span className="text-[12.5px] text-muted">{statusText[c.status]}</span>
                  </div>
                  <p className="text-[12.5px] break-words text-muted">{c.detail}</p>
                </div>
              </li>
            ))}
          </ul>
        )}
        <p className="border-t border-line py-2 text-[12px] text-muted">
          Exported diagnostics contain public keys, endpoints and these events. They never contain private or
          preshared keys.
        </p>
      </Panel>

      <Panel
        title="Events"
        help="events"
        actions={
          <Segmented
            label="Event filter"
            hideLabel
            value={filter}
            onChange={setFilter}
            options={[
              { value: "all", label: "All" },
              { value: "problems", label: "Warnings and errors" },
            ]}
          />
        }
      >
        {shown.length === 0 ? (
          <p className="py-3 text-muted">No events.</p>
        ) : (
          <ol className="selectable max-h-[420px] overflow-y-auto">
            {[...shown].reverse().map((e, i) => (
              <li
                key={`${e.at_ms}-${i}`}
                className="flex gap-3 border-b border-line py-1.5 text-[13px] last:border-b-0"
              >
                <time className="w-20 shrink-0 font-mono text-[12.5px] text-muted">{clock(e.at_ms)}</time>
                <span
                  className={e.level === "error" ? "text-danger" : e.level === "warn" ? "text-pending" : ""}
                >
                  {e.message}
                </span>
              </li>
            ))}
          </ol>
        )}
      </Panel>
    </div>
  );
}
