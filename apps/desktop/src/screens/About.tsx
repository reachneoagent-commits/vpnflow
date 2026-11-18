import { Panel, Row } from "../components/ui";
import { useStore } from "../lib/store";

export function About() {
  const { info, caps } = useStore();
  return (
    <div className="flex max-w-3xl flex-col gap-7">
      <h1 className="text-[28px] leading-tight font-semibold tracking-[-0.015em]">About VPNFlow</h1>
      <Panel>
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
