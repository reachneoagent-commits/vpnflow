import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { Button, InfoTip, Panel, Row } from "../components/ui";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import type { UpdateInfo } from "../lib/types";

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "error"; message: string }
  | { kind: "done"; info: UpdateInfo };

function UpdateRow() {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const check = useCallback(async () => {
    setState({ kind: "checking" });
    try {
      setState({ kind: "done", info: await api.checkForUpdates() });
    } catch (e) {
      const m = (e as { message?: string })?.message;
      setState({ kind: "error", message: m ?? "Could not check for updates." });
    }
  }, []);
  useEffect(() => {
    // The menu bar's "Check for updates" opens this page and asks for a check.
    const p = listen("check-updates", () => void check());
    return () => void p.then((un) => un());
  }, [check]);
  return (
    <div className="flex flex-wrap items-center gap-3 py-3 text-[13px]">
      <Button onClick={() => void check()} disabled={state.kind === "checking"}>
        {state.kind === "checking" ? "Checking…" : "Check for updates"}
      </Button>
      <InfoTip topic="updates" />
      <span role="status" className="text-muted">
        {state.kind === "error" && state.message}
        {state.kind === "done" && !state.info.newer && `You have the latest version (${state.info.current}).`}
        {state.kind === "done" &&
          state.info.newer &&
          `Version ${state.info.latest} is available (you have ${state.info.current}).`}
      </span>
      {state.kind === "done" && state.info.newer && (
        <a
          className="inline-flex h-9 items-center rounded-[10px] bg-accent px-5 font-bold text-accent-fg"
          href={state.info.url}
          target="_blank"
          rel="noreferrer"
        >
          Download
        </a>
      )}
    </div>
  );
}

export function About() {
  const { info, caps } = useStore();
  return (
    <div className="flex flex-col gap-7">
      <h1 className="text-[28px] leading-tight font-semibold tracking-[-0.015em]">About</h1>
      <section className="rounded-xl bg-surface p-6 shadow-(--shadow)">
        <h2 className="text-base font-bold">VPNFlow{info?.version ? ` v${info.version}` : ""}</h2>
        <p className="mt-1.5 text-[13px] text-muted">
          Free, local-first WireGuard client. MIT or Apache-2.0. Part of FlowApps.
        </p>
      </section>
      <Panel title="Updates">
        <UpdateRow />
      </Panel>
      <Panel title="Details">
        <dl>
          <Row label="Version">{info?.version}</Row>
          <Row label="Platform">
            {info?.os} {info?.arch}
          </Row>
          <Row label="Networking backend">
            {caps?.backend === "network_manager"
              ? `NetworkManager ${caps.backend_version ?? ""}`
              : caps?.backend === "windows_service"
                ? "WireGuard tunnel service"
                : caps?.backend === "macos_daemon"
                  ? "VPNFlow macOS daemon"
                  : caps?.backend === "mock"
                    ? "Simulated (no real tunnels)"
                    : "None available"}
          </Row>
          <Row label="License">MIT or Apache-2.0</Row>
          <Row label="Source">
            <a
              className="text-focus underline-offset-2 hover:underline"
              href="https://github.com/reachneoagent-commits/vpnflow"
              target="_blank"
              rel="noreferrer"
            >
              github.com/reachneoagent-commits/vpnflow
            </a>
          </Row>
          <Row label="Security policy">
            <a
              className="text-focus underline-offset-2 hover:underline"
              href="https://github.com/reachneoagent-commits/vpnflow/blob/main/SECURITY.md"
              target="_blank"
              rel="noreferrer"
            >
              SECURITY.md
            </a>
          </Row>
        </dl>
      </Panel>
      <Panel title="Acknowledgments">
        <div className="max-w-[70ch] space-y-2 py-3 text-[13px] leading-relaxed text-muted">
          <p>
            WireGuard® is a registered trademark of Jason A. Donenfeld. VPNFlow is not affiliated with or
            endorsed by the WireGuard project. It uses the WireGuard implementation built into Linux and
            NetworkManager, and on Windows the official WireGuard embeddable tunnel service and WireGuardNT.
          </p>
          <p>
            Built with Tauri, React, Tailwind CSS, Lucide icons, and Rust crates including tokio, serde, zbus,
            notify-rust, ipnet and x25519-dalek. The full dependency and license inventory ships with each
            release.
          </p>
          <p>
            Setup illustrations: Bangalore set by{" "}
            <a
              className="text-focus underline-offset-2 hover:underline"
              href="https://www.streamlinehq.com"
              target="_blank"
              rel="noreferrer"
            >
              Streamline
            </a>
            , licensed CC BY 4.0, recoloured to the VPNFlow palette.
          </p>
          <p>VPNFlow has not been independently security-audited.</p>
        </div>
      </Panel>
      {caps && caps.notes.length > 0 && (
        <Panel title="Backend notes">
          <ul className="list-disc py-3 pl-5 text-[13px] text-muted">
            {caps.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </Panel>
      )}
    </div>
  );
}
