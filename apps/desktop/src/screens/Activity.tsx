import { AlertTriangle, X } from "lucide-react";
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

/** Plain-language next step for a known problem message, or null. */
function adviceFor(message: string): string | null {
  const m = message.toLowerCase();
  if (m.includes("reachability check failed"))
    return "What to do: wait a few seconds and press Run checks. If it keeps failing, disconnect and connect again, or pick a different server in Locations.";
  if (m.includes("missing from this registry"))
    return "What to do: nothing is lost and nothing was changed. If you expected a server here, import its .conf file again in Locations.";
  if (m.includes("kill switch"))
    return "What to do: open Settings, check the Kill switch option, then disconnect and connect again.";
  if (m.includes("helper"))
    return "What to do: press Set up on the banner at the top of the Dashboard and enter your Mac password.";
  if (m.includes("handshake") || m.includes("no response") || m.includes("did not respond"))
    return "What to do: the server did not answer. Switch to another server in Locations; if none work, check your internet connection.";
  return null;
}

export function ActivityScreen() {
  const { events, run, snapshot } = useStore();
  const [checks, setChecks] = useState<Check[] | null>(null);
  const [filter, setFilter] = useState<"all" | "problems">("all");
  // Number of problems already dismissed; the banner returns only for new ones.
  const [dismissed, setDismissed] = useState(0);

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

      {lastProblem && problems.length > dismissed && (
        <div role="alert" className="flex gap-2.5 rounded-xl bg-danger-soft px-4 py-3.5 text-[13px]">
          <AlertTriangle size={18} aria-hidden className="shrink-0 text-danger" />
          <div className="min-w-0 flex-1 break-words">
            <b className="block">
              {problems.length} warning{problems.length > 1 ? "s" : ""} or error
              {problems.length > 1 ? "s" : ""} in the event log
            </b>
            {lastProblem.message}
            {adviceFor(lastProblem.message) && <p className="mt-1.5">{adviceFor(lastProblem.message)}</p>}
          </div>
          <button
            type="button"
            aria-label="Dismiss warning"
            onClick={() => setDismissed(problems.length)}
            className="h-6 w-6 shrink-0 cursor-pointer rounded-md text-muted hover:text-fg"
          >
            <X size={16} aria-hidden className="mx-auto" />
          </button>
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
          <ol className="selectable my-1 max-h-[22rem] overflow-y-auto overscroll-contain">
            <li aria-hidden className="grid grid-cols-[90px_1fr] gap-4 px-1 py-2 text-[11.5px] text-muted">
              <span>Time</span>
              <span>Event</span>
            </li>
            {[...shown].reverse().map((e, i) => (
              <li
                key={`${e.at_ms}-${i}`}
                className="grid grid-cols-[90px_1fr] gap-4 border-t border-line px-1 py-3 text-[13px]"
              >
                <time className="text-[12.5px] text-muted">{clock(e.at_ms)}</time>
                <span
                  className={`min-w-0 break-words ${
                    e.level === "error" ? "text-danger" : e.level === "warn" ? "text-pending" : ""
                  }`}
                >
                  {e.message}
                  {e.level !== "info" && adviceFor(e.message) && (
                    <span className="mt-1 block text-[12.5px] text-muted">{adviceFor(e.message)}</span>
                  )}
                </span>
              </li>
            ))}
          </ol>
        )}
      </Panel>
    </div>
  );
}
