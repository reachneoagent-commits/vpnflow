import { AlertTriangle } from "lucide-react";
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

  const problems = events.filter((e) => e.level !== "info");
  const lastProblem = problems[problems.length - 1];
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

      {lastProblem && (
        <div role="alert" className="flex gap-2.5 rounded-xl bg-danger-soft px-4 py-3.5 text-[13px]">
          <AlertTriangle size={18} aria-hidden className="shrink-0 text-danger" />
          <div>
            <b className="block">
              {problems.length} warning{problems.length > 1 ? "s" : ""} or error
              {problems.length > 1 ? "s" : ""} in the event log
            </b>
            {lastProblem.message}
          </div>
        </div>
      )}

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
                    <span
                      className={`h-[22px] rounded-full px-2.5 text-[11.5px] leading-[22px] font-bold ${
                        c.status === "pass"
                          ? "bg-ok-soft text-ok"
                          : c.status === "warn"
                            ? "bg-pending-soft text-pending"
                            : c.status === "fail"
                              ? "bg-danger-soft text-danger"
                              : "bg-chip text-muted"
                      }`}
                    >
                      {statusText[c.status]}
                    </span>
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
          <ol className="selectable -mx-6 max-h-[420px] overflow-y-auto">
            <li
              aria-hidden
              className="sticky top-0 grid grid-cols-[90px_1fr] gap-4 bg-raised px-6 py-2.5 text-[11.5px] text-muted"
            >
              <span>Time</span>
              <span>Event</span>
            </li>
            {[...shown].reverse().map((e, i) => (
              <li
                key={`${e.at_ms}-${i}`}
                className="grid grid-cols-[90px_1fr] gap-4 border-t border-line px-6 py-3 text-[13px]"
              >
                <time className="text-[12.5px] text-muted">{clock(e.at_ms)}</time>
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
