import { Check, Copy, Download, FileText, KeyRound, Pencil, Plus, Star, Trash2, Upload } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { Button, Confirm, Modal, Segmented, SelectBar, TextField } from "../components/ui";
import { api, asUiError } from "../lib/api";
import { bulkSummary, removeMany } from "../lib/bulk";
import { routeLabel } from "../lib/format";
import { useStore } from "../lib/store";
import type { Issue, KeyAction, PeerDraft, ProfileDraft, ProfileView } from "../lib/types";

export function Profiles() {
  const { profiles, settings, run, saveSettings, refreshProfiles, notify } = useStore();
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<{ id: string | null; draft: ProfileDraft } | null>(null);
  const [deleting, setDeleting] = useState<ProfileView | null>(null);
  const [exporting, setExporting] = useState<ProfileView | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [bulk, setBulk] = useState(false);
  const searchId = useId();

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return profiles;
    return profiles.filter(
      (p) =>
        p.name.toLowerCase().includes(needle) ||
        p.summary.endpoints.some((e) => e.toLowerCase().includes(needle)),
    );
  }, [profiles, q]);

  const pickable = shown.filter((p) => !p.active);
  const chosen = profiles.filter((p) => picked.has(p.id) && !p.active);
  const toggle = (id: string) =>
    setPicked((s) => {
      const n = new Set(s);
      if (!n.delete(id)) n.add(id);
      return n;
    });
  const toggleAll = () =>
    setPicked((s) => (pickable.every((p) => s.has(p.id)) ? new Set() : new Set(pickable.map((p) => p.id))));
  const deleteChosen = async () => {
    setBulk(false);
    const r = await removeMany(chosen, api.deleteProfile);
    setPicked(new Set());
    await refreshProfiles();
    notify({ tone: r.failed.length ? "error" : "info", text: bulkSummary(r) });
  };

  const edit = (id: string) => run(async () => setEditing({ id, draft: await api.draft(id) }));
  const create = () => run(async () => setEditing({ id: null, draft: await api.newDraft() }));
  const setPreferred = (p: ProfileView) =>
    settings && saveSettings({ ...settings, preferred_profile: p.preferred ? null : p.id });

  return (
    <div className="flex flex-col gap-4">
      <h1 className="sr-only">Advanced WireGuard</h1>
      <div>
        <div className="rounded-t-xl bg-(--hero) px-6 py-6 text-(--hero-fg)">
          <h2 className="mb-3 text-base font-bold">Bring your own server</h2>
          <ul className="flex flex-col gap-1.5 text-[12.5px] text-(--hero-muted)">
            {[
              "Import a .conf file from your VPN provider or your own server.",
              "Keys stay on this device and never reach the interface.",
              "Scripts inside profiles are never run.",
            ].map((t) => (
              <li key={t} className="flex items-center gap-3">
                <Check size={16} aria-hidden className="shrink-0 text-brand" />
                {t}
              </li>
            ))}
          </ul>
        </div>
        <div className="divide-y divide-line rounded-b-xl bg-surface shadow-(--shadow)">
          <div className="flex items-center gap-3.5 px-6 py-3.5">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-(--surface-2,var(--raised))">
              <FileText size={18} aria-hidden />
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-bold">Import a WireGuard file</div>
              <div className="text-xs text-muted">
                Drop a .conf here or choose one from disk. You can also drop files onto this window.
              </div>
            </div>
            <Button variant="primary" onClick={() => run(api.importDialog)}>
              <Upload size={15} aria-hidden /> Import .conf
            </Button>
          </div>
          <div className="flex items-center gap-3.5 px-6 py-3.5">
            <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-raised">
              <KeyRound size={18} aria-hidden />
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-bold">Create a profile by hand</div>
              <div className="text-xs text-muted">
                Enter endpoint, keys and peers yourself. Register the public key with your server afterwards.
              </div>
            </div>
            <Button onClick={create}>
              <Plus size={15} aria-hidden /> New profile
            </Button>
          </div>
        </div>
      </div>
      <div className="mt-2 flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-base font-bold">Your profiles</h2>
        <div>
          <label htmlFor={searchId} className="sr-only">
            Search profiles
          </label>
          <input
            id={searchId}
            type="search"
            placeholder="Search by name or endpoint"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            className="w-72 max-w-full"
          />
        </div>
      </div>

      {profiles.length === 0 ? (
        <div className="py-12 text-center">
          <h2 className="text-lg font-semibold">Import a WireGuard file</h2>
          <p className="mx-auto mt-2 max-w-[46ch] text-muted">
            Or create a profile here if you need to enter peers and keys by hand.
          </p>
          <div className="mt-5 flex justify-center gap-2">
            <Button variant="primary" onClick={() => run(api.importDialog)}>
              Import .conf
            </Button>
            <Button onClick={create}>New profile</Button>
          </div>
        </div>
      ) : shown.length === 0 ? (
        <p className="py-6 text-muted">No profile matches “{q}”.</p>
      ) : (
        <>
          <SelectBar
            total={pickable.length}
            selected={chosen.filter((p) => shown.some((s) => s.id === p.id)).length}
            onToggleAll={toggleAll}
            onClear={() => setPicked(new Set())}
            onDelete={() => setBulk(true)}
          />
          <ul className="flex flex-col gap-2">
            {shown.map((p) => (
              <li
                key={p.id}
                className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-[10px] bg-surface px-4 py-3 shadow-(--shadow)"
              >
                <input
                  type="checkbox"
                  aria-label={`Select ${p.name}`}
                  checked={picked.has(p.id)}
                  disabled={p.active}
                  title={p.active ? "Disconnect to select this server" : undefined}
                  onChange={() => toggle(p.id)}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate font-medium">{p.name}</span>
                    {p.active && (
                      <span className="rounded-full bg-ok-soft px-2.5 py-0.5 text-[11.5px] font-bold text-ok">
                        Active
                      </span>
                    )}
                    <span
                      className={`rounded-full px-2.5 py-0.5 text-[11.5px] font-bold ${
                        p.summary.route_kind === "split" ? "bg-chip text-muted" : "bg-chip text-fg"
                      }`}
                    >
                      {routeLabel[p.summary.route_kind]}
                    </span>
                    {p.summary.warnings.length > 0 && (
                      <span className="text-[11.5px] text-pending" title={p.summary.warnings.join("\n")}>
                        {p.summary.warnings.length} warning{p.summary.warnings.length > 1 ? "s" : ""}
                      </span>
                    )}
                  </div>
                  <div className="mt-0.5 truncate font-mono text-[12.5px] text-muted">
                    {p.summary.endpoints.join(", ") || "No endpoint"} · {p.summary.addresses.join(", ")}
                  </div>
                </div>
                <div className="flex items-center gap-1">
                  <IconButton
                    label={p.preferred ? `Unset ${p.name} as preferred` : `Set ${p.name} as preferred`}
                    onClick={() => setPreferred(p)}
                    pressed={p.preferred}
                  >
                    <Star size={15} className={p.preferred ? "fill-accent text-accent" : ""} />
                  </IconButton>
                  <IconButton label={`Edit ${p.name}`} onClick={() => edit(p.id)} disabled={p.active}>
                    <Pencil size={15} />
                  </IconButton>
                  <IconButton
                    label={`Duplicate ${p.name}`}
                    onClick={() => run(() => api.duplicateProfile(p.id), `Duplicated ${p.name}`)}
                  >
                    <Copy size={15} />
                  </IconButton>
                  <IconButton label={`Export ${p.name}`} onClick={() => setExporting(p)}>
                    <Download size={15} />
                  </IconButton>
                  <IconButton label={`Delete ${p.name}`} onClick={() => setDeleting(p)} disabled={p.active}>
                    <Trash2 size={15} />
                  </IconButton>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}

      <Confirm
        open={bulk}
        title={`Delete ${chosen.length} server${chosen.length === 1 ? "" : "s"}?`}
        body={
          <>
            These profiles and their keys are removed from this computer. This cannot be undone unless you
            still have the original .conf files.
            <ul className="mt-2 max-h-40 list-disc overflow-auto pl-5 text-muted">
              {chosen.map((p) => (
                <li key={p.id}>{p.name}</li>
              ))}
            </ul>
          </>
        }
        confirmLabel={`Delete ${chosen.length}`}
        danger
        onCancel={() => setBulk(false)}
        onConfirm={() => void deleteChosen()}
      />
      {editing && (
        <Editor
          key={editing.id ?? "new"}
          id={editing.id}
          initial={editing.draft}
          onClose={() => setEditing(null)}
        />
      )}
      <Confirm
        open={!!deleting}
        title={`Delete ${deleting?.name ?? ""}?`}
        body="The profile and its keys are removed from this computer. This cannot be undone unless you still have the original .conf file."
        confirmLabel="Delete profile"
        danger
        onCancel={() => setDeleting(null)}
        onConfirm={() => {
          const p = deleting;
          setDeleting(null);
          if (p) void run(() => api.deleteProfile(p.id), `Deleted ${p.name}`);
        }}
      />
      <Confirm
        open={!!exporting}
        title="Export includes your private key"
        body={
          <>
            The exported file contains the private key (and preshared key, if any) for{" "}
            <strong>{exporting?.name}</strong>. Anyone with the file can use this VPN identity. Store it
            somewhere private and delete it when no longer needed.
          </>
        }
        confirmLabel="Choose location and export"
        onCancel={() => setExporting(null)}
        onConfirm={() => {
          const p = exporting;
          setExporting(null);
          if (p)
            void run(async () => {
              await api.exportProfile(p.id);
            });
        }}
      />
    </div>
  );
}

function IconButton({
  label,
  children,
  onClick,
  disabled,
  pressed,
}: {
  label: string;
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  pressed?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
      className="rounded-full p-2 text-muted hover:bg-chip hover:text-fg disabled:opacity-40"
    >
      {children}
    </button>
  );
}

const list = (s: string) =>
  s
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);

function numOrNull(s: string): number | null {
  const t = s.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isInteger(n) && n >= 0 && n <= 65535 ? n : Number.NaN;
}

export function Editor({
  id,
  initial,
  onClose,
  onSaved,
}: {
  id: string | null;
  initial: ProfileDraft;
  onClose: () => void;
  onSaved?: (profile: ProfileView) => void;
}) {
  const { run } = useStore();
  const [d, setD] = useState(initial);
  const [text, setText] = useState({
    addresses: initial.addresses.join(", "),
    dns: initial.dns.join(", "),
    mtu: initial.mtu?.toString() ?? "",
    peers: initial.peers.map((p) => ({
      allowed: p.allowed_ips.join(", "),
      keepalive: p.persistent_keepalive?.toString() ?? "",
    })),
  });
  const [issues, setIssues] = useState<Issue[]>([]);
  const [saving, setSaving] = useState(false);
  const err = (field: string, peer?: number) =>
    issues
      .filter(
        (i) =>
          i.field === field &&
          (peer === undefined ||
            i.message.startsWith(`peer ${peer + 1}:`) ||
            i.message.startsWith(`peer ${peer + 1} `)),
      )
      .map((i) => i.message)
      .join("; ") || undefined;

  const setPeer = (i: number, patch: Partial<PeerDraft>) =>
    setD((x) => ({ ...x, peers: x.peers.map((p, k) => (k === i ? { ...p, ...patch } : p)) }));

  const save = async () => {
    const mtu = numOrNull(text.mtu);
    const invalidKeepalive = text.peers.findIndex((p) => Number.isNaN(numOrNull(p.keepalive)));
    if (invalidKeepalive >= 0) {
      setIssues([
        {
          severity: "error",
          line: null,
          field: "PersistentKeepalive",
          message: `Peer ${invalidKeepalive + 1}: enter a whole number from 0 to 65535 seconds, or leave blank.`,
        },
      ]);
      return;
    }
    const draft: ProfileDraft = {
      ...d,
      addresses: list(text.addresses),
      dns: list(text.dns),
      mtu: Number.isNaN(mtu) ? 0 : mtu,
      peers: d.peers.map((p, i) => {
        const ka = numOrNull(text.peers[i]?.keepalive ?? "");
        return {
          ...p,
          allowed_ips: list(text.peers[i]?.allowed ?? ""),
          persistent_keepalive: Number.isNaN(ka) ? null : ka,
        };
      }),
    };
    setSaving(true);
    try {
      const profile = id ? await api.updateProfile(id, draft) : await api.createProfile(draft);
      onClose();
      onSaved?.(profile);
      void run(async () => {}, id ? "Profile saved" : "Profile created");
    } catch (e) {
      const ue = asUiError(e);
      setIssues(
        ue.issues.length ? ue.issues : [{ severity: "error", line: null, field: null, message: ue.message }],
      );
    } finally {
      setSaving(false);
    }
  };

  const general = issues.filter(
    (i) =>
      !i.field || !["Name", "Address", "DNS", "MTU", "PublicKey", "Endpoint", "AllowedIPs"].includes(i.field),
  );

  return (
    <Modal open onClose={onClose} title={id ? `Edit ${initial.name}` : "New profile"} wide>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
        className="flex flex-col gap-4"
      >
        <TextField
          label="Name"
          value={d.name}
          onChange={(name) => setD({ ...d, name })}
          error={err("Name")}
        />
        <fieldset className="rounded-xl border border-line p-4">
          <legend className="px-1 text-[13px] font-semibold">Interface</legend>
          <KeyChoice
            label="Private key"
            value={d.private_key}
            options={id ? ["keep", "generate"] : ["generate"]}
            onChange={(private_key) => setD({ ...d, private_key })}
            hint={
              d.private_key === "keep" && d.public_key
                ? `Public key: ${d.public_key}`
                : "A new key pair is created when you save. Give the new public key to your server administrator."
            }
            error={err("PrivateKey")}
          />
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            <TextField
              label="Addresses"
              mono
              value={text.addresses}
              onChange={(addresses) => setText({ ...text, addresses })}
              placeholder="10.0.0.2/32, fd00::2/128"
              error={err("Address")}
            />
            <TextField
              label="DNS servers"
              mono
              value={text.dns}
              onChange={(dns) => setText({ ...text, dns })}
              placeholder="10.0.0.1"
              hint="Comma separated. Names are used as search domains."
              error={err("DNS")}
            />
            <TextField
              label="MTU"
              value={text.mtu}
              onChange={(mtu) => setText({ ...text, mtu })}
              placeholder="Automatic"
              error={err("MTU")}
            />
          </div>
        </fieldset>

        {d.peers.map((p, i) => (
          <fieldset
            key={`peer-${i}-${p.public_key.slice(0, 6)}`}
            className="rounded-xl border border-line p-4"
          >
            <legend className="px-1 text-[13px] font-semibold">Peer {d.peers.length > 1 ? i + 1 : ""}</legend>
            <div className="grid gap-3 sm:grid-cols-2">
              <TextField
                label="Public key"
                mono
                value={p.public_key}
                onChange={(public_key) => setPeer(i, { public_key })}
                error={err("PublicKey", i)}
              />
              <TextField
                label="Endpoint"
                mono
                value={p.endpoint}
                onChange={(endpoint) => setPeer(i, { endpoint })}
                placeholder="vpn.example.com:51820"
                error={err("Endpoint", i)}
              />
              <TextField
                label="Allowed IPs"
                mono
                value={text.peers[i]?.allowed ?? ""}
                onChange={(allowed) =>
                  setText({ ...text, peers: text.peers.map((x, k) => (k === i ? { ...x, allowed } : x)) })
                }
                hint="0.0.0.0/0, ::/0 sends all traffic through the tunnel."
                error={err("AllowedIPs", i)}
              />
              <TextField
                label="Persistent keepalive (seconds)"
                value={text.peers[i]?.keepalive ?? ""}
                onChange={(keepalive) =>
                  setText({ ...text, peers: text.peers.map((x, k) => (k === i ? { ...x, keepalive } : x)) })
                }
                placeholder="Off"
              />
            </div>
            <div className="mt-3">
              <KeyChoice
                label="Preshared key"
                value={p.preshared_key}
                options={p.has_preshared_key ? ["keep", "generate", "remove"] : ["remove", "generate"]}
                labels={{ remove: p.has_preshared_key ? "Remove" : "None" }}
                onChange={(preshared_key) => setPeer(i, { preshared_key })}
              />
            </div>
            {d.peers.length > 1 && (
              <Button
                variant="ghost"
                className="mt-2 text-danger"
                onClick={() => {
                  setD({ ...d, peers: d.peers.filter((_, k) => k !== i) });
                  setText({ ...text, peers: text.peers.filter((_, k) => k !== i) });
                }}
              >
                Remove peer
              </Button>
            )}
          </fieldset>
        ))}
        <div>
          <Button
            onClick={() => {
              setD({
                ...d,
                peers: [
                  ...d.peers,
                  {
                    public_key: "",
                    preshared_key: "remove",
                    has_preshared_key: false,
                    endpoint: "",
                    allowed_ips: [],
                    persistent_keepalive: null,
                  },
                ],
              });
              setText({ ...text, peers: [...text.peers, { allowed: "", keepalive: "" }] });
            }}
          >
            Add peer
          </Button>
        </div>

        {general.length > 0 && (
          <ul role="alert" className="rounded-md bg-danger-soft px-3 py-2 text-[13px] text-danger">
            {general.map((g) => (
              <li key={g.message}>{g.field ? `${g.field}: ${g.message}` : g.message}</li>
            ))}
          </ul>
        )}
        <div className="flex justify-end gap-2 border-t border-line pt-4">
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={saving}>
            {id ? "Save changes" : "Create profile"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

function KeyChoice({
  label,
  value,
  options,
  labels,
  onChange,
  hint,
  error,
}: {
  label: string;
  value: KeyAction;
  options: KeyAction[];
  labels?: Partial<Record<KeyAction, string>>;
  onChange: (v: KeyAction) => void;
  hint?: string;
  error?: string;
}) {
  const names: Record<KeyAction, string> = {
    keep: "Keep current",
    generate: "Generate new",
    remove: "None",
    ...labels,
  };
  return (
    <div>
      <Segmented
        label={label}
        value={value}
        onChange={onChange}
        options={options.map((o) => ({ value: o, label: names[o] }))}
      />
      {(error || hint) && (
        <p className={`mt-1 break-all text-[12px] ${error ? "text-danger" : "text-muted"}`}>
          {error ?? hint}
        </p>
      )}
      <p className="mt-1 text-[12px] text-muted">
        Keys are never shown or typed here. To use an existing key, import its .conf file.
      </p>
    </div>
  );
}
