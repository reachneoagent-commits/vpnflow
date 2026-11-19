import { useEffect, useState } from "react";
import { api, asUiError, type HelperStatus } from "../lib/api";
import { Button, InfoTip } from "./ui";

/** Shown on a Mac whose background helper is missing or from an older build. */
export function HelperBanner() {
  const [status, setStatus] = useState<HelperStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .helperStatus()
      .then((s) => live && setStatus(s))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  if (status !== "missing" && status !== "outdated") return null;

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.installHelper();
    } catch (e) {
      setError(asUiError(e).message);
      setBusy(false);
    }
  };

  return (
    <section
      aria-label="One-time setup"
      className="mx-6 mt-4 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-xl bg-pending-soft px-5 py-3.5 text-[13px] text-fg"
    >
      <span className="min-w-0 flex-1">
        <b className="block text-pending">{status === "outdated" ? "Update needed" : "Setup needed"}</b>
        {status === "outdated"
          ? "VPNFlow needs to update its helper before it can connect. You will be asked for your Mac password."
          : "VPNFlow needs a one-time setup before it can connect. You will be asked for your Mac password."}
      </span>
      <InfoTip topic="helperSetup" />
      <Button variant="primary" onClick={run} disabled={busy}>
        {busy ? "Setting up..." : status === "outdated" ? "Update" : "Set up"}
      </Button>
      {error && <span role="alert">{error}</span>}
    </section>
  );
}
