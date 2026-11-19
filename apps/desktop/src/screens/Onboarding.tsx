import { Check, ExternalLink } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import accountArt from "../assets/onboarding/account.svg?raw";
import countriesArt from "../assets/onboarding/countries.svg?raw";
import finishArt from "../assets/onboarding/finish.svg?raw";
import serverFilesArt from "../assets/onboarding/server-files.svg?raw";
import welcomeArt from "../assets/onboarding/welcome.svg?raw";
import { Shield } from "../components/Shield";
import { Button, Panel, Switch } from "../components/ui";
import { api } from "../lib/api";
import { countryName } from "../lib/locations";
import { useStore } from "../lib/store";
import { CountryFlag } from "./Locations";

const STEPS = ["Welcome", "Account", "Server files", "Countries", "Finish"] as const;
const KEY = "vpnflow.onboarding.step";
// AdGuard DNS: blocks ads and trackers. Same servers as the Settings preset.
const ADGUARD_DNS = ["94.140.14.14", "94.140.15.15"];

function savedStep(): number {
  try {
    const n = Number(localStorage.getItem(KEY));
    return Number.isInteger(n) && n >= 0 && n < STEPS.length ? n : 0;
  } catch {
    return 0;
  }
}

function LinkButton({ which, children }: { which: string; children: ReactNode }) {
  const { run } = useStore();
  return (
    <Button onClick={() => void run(() => api.openLink(which))}>
      {children}
      <ExternalLink size={14} aria-hidden />
    </Button>
  );
}

function Checklist({ items }: { items: string[] }) {
  return (
    <ol className="mt-3 list-decimal space-y-1.5 pl-5 text-[14px]">
      {items.map((t) => (
        <li key={t}>{t}</li>
      ))}
    </ol>
  );
}

export function Onboarding({ onDone }: { onDone: () => void }) {
  const { profiles, countries, settings, saveSettings, refreshProfiles, run, caps } = useStore();
  const [step, setStep] = useState(savedStep);
  const startedAt = useRef(Date.now() - 10 * 60_000);
  const [imported, setImported] = useState<string[]>([]);
  const [failed, setFailed] = useState(0);
  const [stuck, setStuck] = useState(false);

  const go = useCallback((n: number) => {
    setStep(n);
    try {
      localStorage.setItem(KEY, String(n));
    } catch {
      /* remembering the step is optional */
    }
  }, []);

  // Recommended protection starts on at the last step; the user can still turn it off.
  const protectionSeeded = useRef(false);
  useEffect(() => {
    if (step !== 4 || protectionSeeded.current || !settings || !caps) return;
    protectionSeeded.current = true;
    const next = { ...settings };
    if (caps.kill_switch.status === "supported") {
      next.kill_switch = true;
      next.kill_switch_allow_lan = true;
    }
    if (caps.backend === "macos_daemon" && settings.dns_servers.length === 0) {
      next.dns_servers = ADGUARD_DNS;
    }
    if (JSON.stringify(next) !== JSON.stringify(settings)) void saveSettings(next);
  }, [step, settings, caps, saveSettings]);

  // Watch Downloads for new server files while that step is showing.
  useEffect(() => {
    if (step !== 2) return;
    let alive = true;
    const t0 = Date.now();
    const tick = async () => {
      try {
        const r = await api.scanDownloads(startedAt.current);
        if (!alive) return;
        if (r.imported.length) {
          setImported((xs) => [...xs, ...r.imported]);
          await refreshProfiles();
        }
        if (r.failed) setFailed((n) => n + r.failed);
      } catch {
        /* the manual button below still works */
      }
      if (alive && Date.now() - t0 > 60_000) setStuck(true);
    };
    void tick();
    const timer = setInterval(() => void tick(), 3000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [step, refreshProfiles]);

  if (!settings) return null;

  const finish = async () => {
    const first = profiles.find((p) => p.server.enabled);
    await saveSettings({
      ...settings,
      onboarded: true,
      preferred_profile: settings.preferred_profile ?? first?.id ?? null,
    });
    try {
      localStorage.removeItem(KEY);
    } catch {
      /* ignore */
    }
    onDone();
  };

  const skip = async () => {
    await saveSettings({ ...settings, onboarded: true });
    onDone();
  };

  // Leave setup at any step. Anything already imported or chosen stays.
  const cancel = async () => {
    if (!settings.onboarded) await saveSettings({ ...settings, onboarded: true });
    try {
      localStorage.removeItem(KEY);
    } catch {
      /* ignore */
    }
    onDone();
  };

  const chooseFiles = async () => {
    await run(async () => {
      const added = await api.importDialog();
      setImported((xs) => [...xs, ...added.map((p) => p.name)]);
      await refreshProfiles();
    });
  };

  const enabledCount = profiles.filter((p) => p.server.enabled).length;
  const byCountry = new Map<string, typeof profiles>();
  for (const p of profiles) {
    const code = p.server.location?.country_code ?? "";
    byCountry.set(code, [...(byCountry.get(code) ?? []), p]);
  }

  const setCountryEnabled = async (code: string, enabled: boolean) => {
    for (const p of byCountry.get(code) ?? []) {
      const { location, favorite, registration_pending } = p.server;
      await run(() => api.updateServer(p.id, { location, favorite, enabled, registration_pending }));
    }
    await refreshProfiles();
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface">
      <header className="flex h-[60px] shrink-0 items-center gap-2.5 border-b border-line px-6">
        <Shield size={30} />
        <span className="text-[19px] leading-none font-bold tracking-[-0.02em]">VPNFlow</span>
      </header>
      <div className="mx-auto my-auto flex h-[min(780px,100%)] w-full max-w-[1040px] flex-col px-8 py-8">
        <div className="mb-8 shrink-0">
          <p className="mb-2.5 text-xs font-semibold text-muted">
            Step {step + 1} of {STEPS.length} · {STEPS[step]}
          </p>
          <ol className="flex gap-1.5" aria-label="Setup progress">
            {STEPS.map((s, i) => (
              <li
                key={s}
                aria-current={i === step ? "step" : undefined}
                className={`h-1.5 flex-1 rounded-full ${i <= step ? "bg-accent" : "bg-chip"}`}
              >
                <span className="sr-only">{s}</span>
              </li>
            ))}
          </ol>
        </div>

        <div className="grid min-h-0 flex-1 gap-12 min-[860px]:grid-cols-[minmax(0,1fr)_280px]">
          <div className="flex h-full min-h-0 min-w-0 max-w-[640px] flex-col">
            {step === 0 && (
              <section className="flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
                <h1 className="text-2xl font-bold">Welcome to VPNFlow</h1>
                <p className="mt-3 text-[15px] text-muted">
                  VPNFlow is a free VPN app that keeps your data on your computer. It connects through the
                  free servers from Proton VPN. This setup takes about five minutes and gets everything ready,
                  so you can connect with one click afterwards.
                </p>
                <ul className="mt-4 list-disc space-y-1 pl-5 text-[14px]">
                  <li>You make a free Proton account (VPNFlow never sees your password).</li>
                  <li>You download your server files from Proton. VPNFlow picks them up for you.</li>
                  <li>You choose which countries you want, and we finish the setup.</li>
                </ul>
                <div className="mt-auto flex flex-wrap items-center gap-3 pt-6">
                  <Button variant="primary" onClick={() => go(1)}>
                    Get started
                  </Button>
                  <Button variant="ghost" onClick={() => void skip()}>
                    Skip, I already have server files
                  </Button>
                  <Button variant="ghost" className="ml-auto" onClick={() => void cancel()}>
                    Cancel setup
                  </Button>
                </div>
              </section>
            )}

            {step === 1 && (
              <section className="flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
                <h1 className="text-2xl font-bold">Make your free Proton account</h1>
                <p className="mt-2 text-[14px] text-muted">
                  This opens in your normal web browser. Come back here when you are done.
                </p>
                <Checklist
                  items={[
                    "Click the button below to open Proton's sign-up page.",
                    "Choose the free plan and fill in a username and password.",
                    "Proton emails you a code. Enter it to verify your email.",
                    "Log in once to make sure it works, then return to VPNFlow.",
                  ]}
                />
                <div className="mt-5 flex flex-wrap gap-2">
                  <LinkButton which="signup">Open Proton sign-up</LinkButton>
                  <LinkButton which="login">I already have an account: log in</LinkButton>
                </div>
                <Nav
                  cancel={() => void cancel()}
                  back={() => go(0)}
                  next={() => go(2)}
                  nextLabel="I have an account"
                />
              </section>
            )}

            {step === 2 && (
              <section className="flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
                <h1 className="text-2xl font-bold">Download your server files</h1>
                <p className="mt-2 text-[14px] text-muted">
                  VPNFlow is watching your Downloads folder. Each file you download from Proton is added
                  automatically.
                </p>
                <Checklist
                  items={[
                    "Click the button below. Proton opens its WireGuard page. Sign in if asked.",
                    'Step 1, name: type a name such as "VPNFlow". Use the same name for every server.',
                    "Step 2, platform: pick the one for this computer (macOS, Windows or GNU/Linux).",
                    "Step 3, options: leave NAT-PMP and VPN Accelerator as they are.",
                    'Step 4, server: click the "Free server configs" tab. Each country is a button.',
                    "Click a country you want, then Create, then Download. Repeat for each country (the free ones are Canada, Japan, Mexico, Netherlands, Norway, Poland, Romania, Singapore, Switzerland and the United States).",
                    "Return here. Your servers appear below within a few seconds.",
                  ]}
                />
                <div className="mt-5 flex flex-wrap gap-2">
                  <LinkButton which="wireguard">Open Proton downloads</LinkButton>
                  <LinkButton which="help">Help from Proton</LinkButton>
                </div>
                <div className="mt-6" role="status" aria-live="polite">
                  {imported.length > 0 ? (
                    <p className="flex items-center gap-2 text-[14px] font-medium text-ok">
                      <Check size={16} aria-hidden /> {imported.length} server
                      {imported.length === 1 ? "" : "s"} added: {imported.slice(-4).join(", ")}
                      {imported.length > 4 ? "…" : ""}
                    </p>
                  ) : (
                    <p className="text-[14px] text-muted">Waiting for a downloaded file…</p>
                  )}
                  {failed > 0 && (
                    <p className="mt-1 text-[13px] text-muted">
                      {failed} file{failed === 1 ? " was" : "s were"} not a usable server file and skipped.
                    </p>
                  )}
                </div>
                {(stuck || imported.length === 0) && (
                  <div className="mt-3">
                    <Button onClick={() => void chooseFiles()}>
                      {stuck ? "Nothing yet? Choose files manually" : "Choose files manually"}
                    </Button>
                  </div>
                )}
                <Nav
                  cancel={() => void cancel()}
                  back={() => go(1)}
                  next={() => go(3)}
                  nextLabel="Continue"
                  nextDisabled={profiles.length === 0}
                />
              </section>
            )}

            {step === 3 && (
              <section className="flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
                <h1 className="text-2xl font-bold">Choose your countries</h1>
                <p className="mt-2 text-[14px] text-muted">
                  Tick the countries you want to use. You can change this later in Locations.
                </p>
                <Panel className="mt-4">
                  <ul>
                    {[...byCountry.entries()].map(([code, ps]) => {
                      const on = ps.some((p) => p.server.enabled);
                      const name = code ? countryName(code, countries) : "Unassigned location";
                      return (
                        <li
                          key={code || "none"}
                          className="flex items-center gap-3 border-b border-line py-2.5 last:border-b-0"
                        >
                          <CountryFlag code={code} />
                          <label className="flex flex-1 items-center gap-3">
                            <input
                              type="checkbox"
                              checked={on}
                              onChange={(e) => void setCountryEnabled(code, e.target.checked)}
                            />
                            <span className="font-medium">{name}</span>
                            <span className="text-[12px] text-muted">
                              {ps.length} server{ps.length === 1 ? "" : "s"}
                            </span>
                          </label>
                        </li>
                      );
                    })}
                  </ul>
                </Panel>
                <Nav
                  cancel={() => void cancel()}
                  back={() => go(2)}
                  next={() => go(4)}
                  nextLabel="Continue"
                  nextDisabled={enabledCount === 0}
                />
              </section>
            )}

            {step === 4 && (
              <section className="flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
                <h1 className="text-2xl font-bold">Almost done</h1>
                <p className="mt-2 text-[14px] text-muted">
                  We switched on the settings we recommend. Turn any of them off here, or change them later in
                  Settings.
                </p>
                <Panel className="mt-4">
                  {caps?.backend === "macos_daemon" && (
                    <Switch
                      label="Block ads and trackers"
                      description="Uses AdGuard to stop most ads and trackers before they load, in every app. It cannot catch every ad. More choices in Settings. Recommended."
                      checked={settings.dns_servers.length > 0}
                      onChange={(v) => void saveSettings({ ...settings, dns_servers: v ? ADGUARD_DNS : [] })}
                    />
                  )}
                  <Switch
                    label="Kill switch"
                    description="Blocks the internet if the VPN drops, so nothing leaks. Recommended."
                    checked={settings.kill_switch}
                    onChange={(v) => void saveSettings({ ...settings, kill_switch: v })}
                  />
                  <Switch
                    label="Allow local network"
                    description="Keeps printers, your router and apps on your own network working while the VPN is on."
                    checked={settings.kill_switch_allow_lan}
                    disabled={!settings.kill_switch}
                    onChange={(v) => void saveSettings({ ...settings, kill_switch_allow_lan: v })}
                  />
                  <Switch
                    label="Connect when VPNFlow starts"
                    description="Connects automatically each time you open the app."
                    checked={settings.connect_on_launch}
                    onChange={(v) => void saveSettings({ ...settings, connect_on_launch: v })}
                  />
                  <Switch
                    label="Start VPNFlow when you sign in"
                    checked={settings.launch_on_login}
                    onChange={(v) => void saveSettings({ ...settings, launch_on_login: v })}
                  />
                </Panel>
                <Nav
                  cancel={() => void cancel()}
                  back={() => go(3)}
                  next={() => void finish()}
                  nextLabel="Finish setup"
                />
              </section>
            )}
          </div>
          <StepArt step={step} />
        </div>
      </div>
    </div>
  );
}

const LINES = [
  "Free, private and local. Your data stays on this computer.",
  "One free Proton account. VPNFlow never sees your password.",
  "Server files you download are picked up automatically.",
  "Choose the countries you want. Change them any time.",
  "Recommended protection is switched on for you.",
];

/** Streamline Bangalore (CC BY 4.0), recoloured to the brand through `.ill` in styles.css. */
const SCENES: string[] = [welcomeArt, accountArt, serverFilesArt, countriesArt, finishArt];

/** Brand illustration beside the steps; decorative only. */
function StepArt({ step }: { step: number }) {
  const i = Math.min(step, SCENES.length - 1);
  const art = SCENES[i] ?? "";
  return (
    <aside aria-hidden className="hidden flex-col items-center gap-3 pt-6 min-[860px]:flex">
      <div
        className="ill w-full max-w-[280px]"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: bundled static SVG files, no user input
        dangerouslySetInnerHTML={{ __html: art }}
      />
      <p className="max-w-[24ch] text-center text-[13px] leading-snug text-muted">{LINES[i]}</p>
    </aside>
  );
}

function Nav({
  cancel,
  back,
  next,
  nextLabel,
  nextDisabled,
}: {
  cancel: () => void;
  back: () => void;
  next: () => void;
  nextLabel: string;
  nextDisabled?: boolean;
}) {
  return (
    <div className="mt-auto flex flex-wrap items-center gap-3 pt-6">
      <Button variant="ghost" onClick={back}>
        Back
      </Button>
      <Button variant="primary" onClick={next} disabled={nextDisabled}>
        {nextLabel}
      </Button>
      <Button variant="ghost" className="ml-auto" onClick={cancel}>
        Cancel setup
      </Button>
    </div>
  );
}
