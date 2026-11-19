import { Panel, Row } from "../components/ui";
import { useStore } from "../lib/store";

export function About() {
  const { info, caps } = useStore();
  return (
    <div className="flex max-w-3xl flex-col gap-7">
      <h1 className="text-[28px] leading-tight font-semibold tracking-[-0.015em]">About</h1>
      <section className="rounded-xl bg-surface p-6 shadow-(--shadow)">
        <h2 className="text-base font-bold">VPNFlow{info?.version ? ` v${info.version}` : ""}</h2>
        <p className="mt-1.5 text-[13px] text-muted">
          Free, local-first WireGuard client. MIT or Apache-2.0. Part of FlowApps.
        </p>
      </section>
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
