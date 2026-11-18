import { ChevronDown, ChevronRight, Globe, Plus, RefreshCw, Star, Upload, Zap } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { Button, Confirm, Dot, Modal, Segmented, TextField } from "../components/ui";
import { api, asUiError } from "../lib/api";
import { isActive, isBusy, stateHeadline, stateTone } from "../lib/connection";
import { ago } from "../lib/format";
import {
  hasRecentResponse,
  type LocationFilter,
  type LocationSort,
  locationGroups,
  locationLabel,
} from "../lib/locations";
import { useStore } from "../lib/store";
import type { ProfileDraft, ProfileView, ServerUpdate } from "../lib/types";
import { Editor } from "./Profiles";

export function CountryFlag({ code }: { code?: string }) {
  return code && /^[A-Z]{2}$/.test(code) ? (
    <img
      src={`/flags/${code.toLowerCase()}.svg`}
      alt=""
      width={28}
      height={21}
      className="h-[21px] w-7 shrink-0 rounded-sm object-cover ring-1 ring-line"
    />
  ) : (
    <Globe size={24} aria-hidden className="shrink-0 text-muted" />
  );
}

export function Locations() {
  const { profiles, countries, snapshot, settings, run, saveSettings, caps, refreshProfiles } = useStore();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<LocationFilter>("all");
  const [sort, setSort] = useState<LocationSort>("country");
  const [country, setCountry] = useState<string | null | undefined>(undefined);
  const [adding, setAdding] = useState(false);
  const [assigning, setAssigning] = useState<ProfileView[]>([]);
  const [editor, setEditor] = useState<{ id: string | null; draft: ProfileDraft } | null>(null);
  const [deleting, setDeleting] = useState<ProfileView | null>(null);
  const [managing, setManaging] = useState<ProfileView | null>(null);
  const [working, setWorking] = useState(false);
  const [guide, setGuide] = useState(false);
  const [now, setNow] = useState(Date.now());
  const searchId = useId();
  const sortId = useId();
  const countryPreferenceId = useId();
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);
  if (!settings || !snapshot) return null;
  const state = snapshot.state;
  const busy = working || isBusy(state) || state.state === "reconnecting";
  const unsafe = state.state === "uncertain";
  const activeId = isActive(state) && "profile" in state ? state.profile : null;
  const groups = locationGroups(profiles, countries, settings, snapshot.health, query, filter, sort, now);
  const selectedCode =
    country === null
      ? null
      : country !== undefined && groups.some((g) => g.code === country)
        ? country
        : (groups.find((g) => g.servers.some((p) => p.id === activeId)) ?? groups[0])?.code;
  const selected = groups.find((g) => g.code === selectedCode);
  const active = profiles.find((p) => p.id === activeId);
  const perform = async (action: () => Promise<unknown>) => {
    setWorking(true);
    try {
      await run(action);
    } finally {
      setWorking(false);
      setNow(Date.now());
    }
  };
  const importServers = () =>
    perform(async () => {
      const imported = await api.importDialog();
      await refreshProfiles();
      if (imported.length) {
        setAdding(false);
        setAssigning(imported);
      }
    });
  const create = () =>
    perform(async () => {
      setEditor({ id: null, draft: await api.newDraft() });
      setAdding(false);
    });
  const edit = (p: ProfileView) => perform(async () => setEditor({ id: p.id, draft: await api.draft(p.id) }));
  const favorite = (p: ProfileView) =>
    perform(() => api.updateServer(p.id, { ...p.server, favorite: !p.server.favorite }));
  const favoriteCountry = (code: string) =>
    saveSettings({
      ...settings,
      favorite_countries: settings.favorite_countries.includes(code)
        ? settings.favorite_countries.filter((c) => c !== code)
        : [...settings.favorite_countries, code],
    });

  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-[30px] font-semibold leading-tight tracking-[-0.015em]">VPN Locations</h1>
          <p className="mt-1.5 max-w-[54ch] text-muted">
            Pick a server you configure. VPNFlow connects with the WireGuard profile you imported or created.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy || unsafe} onClick={() => perform(api.quickConnect)}>
            <Zap size={15} aria-hidden />
            Quick Connect
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => setAdding(true)}>
            <Plus size={15} aria-hidden />
            Add server
          </Button>
        </div>
      </header>
      <div className="flex flex-wrap items-end gap-3">
        <div className="min-w-48 flex-1">
          <label className="sr-only" htmlFor={searchId}>
            Search locations
          </label>
          <input
            id={searchId}
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search country, city, server or endpoint"
            className="w-full rounded-lg border border-line bg-surface px-3 py-2"
          />
        </div>
        <div>
          <label htmlFor={sortId} className="sr-only">
            Sort locations
          </label>
          <select
            id={sortId}
            value={sort}
            onChange={(e) => setSort(e.target.value as LocationSort)}
            className="rounded-lg border border-line bg-surface px-2 py-2"
          >
            <option value="country">Country name</option>
            <option value="recent">Recently connected</option>
            <option value="favorites">Favorites first</option>
          </select>
        </div>
        <Button
          disabled={busy}
          onClick={() => perform(api.refreshServerHealth)}
          title="Read active tunnel status; other endpoints are not probed"
        >
          <RefreshCw size={15} aria-hidden />
          Refresh status
        </Button>
      </div>
      <Segmented
        label="Filter locations"
        hideLabel
        value={filter}
        onChange={setFilter}
        options={[
          { value: "all", label: "All countries" },
          { value: "favorites", label: "Favorites" },
          { value: "recent", label: "Recently used" },
          { value: "available", label: "Available servers" },
        ]}
      />
      {profiles.length === 0 ? (
        <section className="px-2 py-14 text-center">
          <Globe size={36} className="mx-auto mb-4 text-muted" aria-hidden />
          <h2 className="text-xl font-semibold">Import a WireGuard file</h2>
          <p className="mx-auto mt-2 max-w-[48ch] text-muted">
            Bring in a configuration from your provider or server, then assign a country so it shows up here.
          </p>
          <div className="mt-6 flex flex-wrap justify-center gap-2">
            <Button variant="primary" disabled={busy} onClick={importServers}>
              Import WireGuard Configuration
            </Button>
            <Button onClick={() => setAdding(true)}>Add Server</Button>
            <Button variant="ghost" onClick={() => setGuide(true)}>
              View Setup Guide
            </Button>
          </div>
        </section>
      ) : groups.length === 0 ? (
        <div className="px-1 py-8" role="status">
          <p className="font-medium">
            {filter === "available"
              ? "No servers have answered recently"
              : filter === "favorites"
                ? "No favorite servers yet"
                : filter === "recent"
                  ? "No recently used servers"
                  : "No servers match this filter"}
          </p>
          <p className="mt-2 max-w-[58ch] text-muted">
            {filter === "available"
              ? "Available only lists servers that returned an authenticated WireGuard response in the last five minutes while a tunnel was up. Idle servers are not probed. Try All countries, or connect once so a response can be recorded."
              : "Try All countries, clear the search, or add a server."}
          </p>
        </div>
      ) : (
        <div className="grid items-start gap-5 min-[850px]:grid-cols-[230px_minmax(0,1fr)]">
          <section aria-label="Country directory" className="rounded-xl border border-line bg-surface">
            <h2 className="border-b border-line px-4 py-3 font-semibold">
              Countries <span className="text-muted">{groups.length}</span>
            </h2>
            <ul className="divide-y divide-line">
              {groups.map((g) => (
                <li key={g.code} className="flex items-start gap-1 p-2">
                  <button
                    type="button"
                    aria-expanded={selectedCode === g.code}
                    aria-controls="location-servers"
                    onClick={() => setCountry(selectedCode === g.code ? null : g.code)}
                    className={`flex min-w-0 flex-1 items-center gap-3 rounded-lg px-2 py-2 text-left ${selectedCode === g.code ? "bg-raised" : "hover:bg-raised/60"}`}
                  >
                    <CountryFlag code={g.code} />
                    <span className="min-w-0 flex-1">
                      <span className="flex items-center gap-2 font-medium">
                        {g.name}
                        {g.servers.some((p) => p.id === activeId) && <Dot tone={stateTone(state)} />}
                      </span>
                      <span className="block text-xs text-muted">
                        {g.servers.length} server{g.servers.length !== 1 ? "s" : ""}
                      </span>
                      {g.responded > 0 && (
                        <span className="block text-xs text-muted">{g.responded} responded recently</span>
                      )}
                    </span>
                    {selectedCode === g.code ? (
                      <ChevronDown size={14} aria-hidden />
                    ) : (
                      <ChevronRight size={14} aria-hidden />
                    )}
                  </button>
                  {g.code && (
                    <button
                      type="button"
                      aria-label={`Favorite country ${g.name}`}
                      aria-pressed={g.favorite}
                      disabled={busy}
                      onClick={() => void favoriteCountry(g.code)}
                      className="mt-2 rounded p-1 text-muted"
                    >
                      <Star size={14} className={g.favorite ? "fill-current text-focus" : ""} aria-hidden />
                    </button>
                  )}
                </li>
              ))}
            </ul>
          </section>
          <section id="location-servers" aria-label="Available servers" className="min-w-0">
            <div className="mb-3 flex items-baseline justify-between gap-3">
              <h2 className="text-lg font-semibold">{selected?.name ?? "Select a country"}</h2>
              <span className="text-xs text-muted">{selected?.servers.length ?? 0} configured</span>
            </div>
            <ul className="flex flex-col gap-3">
              {selected?.servers.map((p) => {
                const health = snapshot.health.find((h) => h.id === p.id);
                const current = p.id === activeId;
                const responded = hasRecentResponse(health, now);
                const confirmed = current && state.state === "connected" && state.peer === "confirmed";
                const failed = state.state === "failed" && state.profile === p.id;
                const status = failed
                  ? "Last connection failed"
                  : current
                    ? stateHeadline(state, p.name)
                    : p.server.registration_pending
                      ? "Remote key registration pending"
                      : !p.server.enabled
                        ? "Disabled"
                        : responded
                          ? "Responded recently"
                          : "Reachability unknown";
                return (
                  <li
                    key={p.id}
                    className={`min-w-0 rounded-xl border bg-surface p-4 ${current ? "border-focus" : "border-line"}`}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <h3 className="flex items-center gap-2 text-base font-semibold">
                          <CountryFlag code={p.server.location?.country_code} />
                          {p.name}
                        </h3>
                        <p className="mt-1 text-[13px] text-muted">{locationLabel(p, countries)}</p>
                        {p.server.location && (
                          <p className="text-[12px] text-muted">User-assigned location</p>
                        )}
                      </div>
                      <div className="flex items-start gap-1">
                        <button
                          type="button"
                          aria-label={`Favorite ${p.name}`}
                          aria-pressed={p.server.favorite}
                          disabled={busy}
                          onClick={() => favorite(p)}
                          className="rounded p-1.5 text-muted"
                        >
                          <Star
                            size={16}
                            aria-hidden
                            className={p.server.favorite ? "fill-current text-focus" : ""}
                          />
                        </button>
                        <Button
                          variant="ghost"
                          className="px-2 py-1 text-[13px] text-muted"
                          aria-label={`Manage ${p.name}`}
                          onClick={() => setManaging(p)}
                        >
                          Manage
                        </Button>
                      </div>
                    </div>
                    <p className="selectable mt-3 break-all font-mono text-[12px] text-muted">
                      {p.summary.endpoints.join(", ") || "Missing endpoint"}
                    </p>
                    <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
                      <div>
                        <dt className="text-muted">Protocol</dt>
                        <dd>WireGuard</dd>
                        {p.summary.peer_count > 1 && (
                          <dd className="text-muted">{p.summary.peer_count} peers (preserved)</dd>
                        )}
                      </div>
                      <div>
                        <dt className="text-muted">Last successful connection</dt>
                        <dd>
                          {p.server.last_connected_ms
                            ? ago(p.server.last_connected_ms, now)
                            : "None recorded"}
                        </dd>
                      </div>
                      <div>
                        <dt className="text-muted">Last response</dt>
                        <dd>{health?.observed_ms ? ago(health.observed_ms, now) : "Not observed"}</dd>
                      </div>
                    </dl>
                    {failed && state.state === "failed" && (
                      <p className="mt-3 text-[13px] text-danger" role="alert">
                        {state.reason}
                      </p>
                    )}
                    <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-line pt-3">
                      <span className="flex items-center gap-2 text-[13px]" role="status">
                        <Dot tone={failed ? "bad" : confirmed ? "ok" : current ? stateTone(state) : "none"} />
                        {status}
                      </span>
                      {current ? (
                        <Button
                          disabled={state.state === "disconnecting"}
                          onClick={() =>
                            perform(
                              state.state === "connecting" ||
                                state.state === "preparing" ||
                                state.state === "reconnecting"
                                ? api.cancel
                                : api.disconnect,
                            )
                          }
                        >
                          {["preparing", "connecting", "reconnecting"].includes(state.state)
                            ? "Cancel"
                            : "Disconnect"}
                        </Button>
                      ) : (
                        <Button
                          variant="primary"
                          disabled={
                            busy ||
                            unsafe ||
                            !p.server.enabled ||
                            p.server.registration_pending ||
                            caps?.available.status !== "supported"
                          }
                          onClick={() => perform(() => api.connect(p.id))}
                        >
                          {activeId ? "Switch server" : "Connect"}
                        </Button>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          </section>
        </div>
      )}
      <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-4 text-[13px]">
        {!activeId && snapshot.protection.kill_switch.status === "verified" && (
          <Button disabled={working} onClick={() => perform(api.disconnect)}>
            Stop blocking
          </Button>
        )}
        <span className="flex min-w-0 flex-col gap-0.5" aria-live="polite">
          <span className="flex items-center gap-2">
            <Dot tone={stateTone(state)} />
            {active ? stateHeadline(state, active.name) : "No active server"}
          </span>
          {active && <span className="pl-5 text-xs text-muted">{locationLabel(active, countries)}</span>}
        </span>
        <div className="flex items-center gap-2">
          <label htmlFor={countryPreferenceId} className="text-muted">
            Quick Connect country
          </label>
          <select
            id={countryPreferenceId}
            value={settings.quick_connect_country ?? ""}
            disabled={busy}
            onChange={(e) =>
              void saveSettings({ ...settings, quick_connect_country: e.target.value || null })
            }
            className="max-w-44 rounded border border-line bg-surface p-1.5"
          >
            <option value="">Any country</option>
            {countries.map((c) => (
              <option key={c.code} value={c.code}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
      </footer>
      <p className="text-xs text-muted">
        Status uses tunnel observations, not internet reachability. Idle servers are not probed. Quick Connect
        uses recent responses and your preferences; it does not measure download speed.
      </p>
      {adding && (
        <Modal open title="Add server" onClose={() => setAdding(false)}>
          <p className="mb-4 text-[13px] text-muted">
            Use a configuration from your provider or an existing WireGuard server. Importing does not
            configure the remote server.
          </p>
          <div className="flex flex-col gap-3">
            <Button variant="primary" disabled={busy} onClick={importServers}>
              <Upload size={15} aria-hidden />
              Import WireGuard Configuration
            </Button>
            <Button disabled={busy} onClick={create}>
              Configure manually
            </Button>
          </div>
          {profiles.length > 0 && (
            <div className="mt-5">
              <h3 className="mb-2 font-medium">Assign an existing profile</h3>
              <ul className="flex max-h-52 flex-col gap-1 overflow-auto">
                {profiles.map((p) => (
                  <li key={p.id}>
                    <Button
                      variant="ghost"
                      className="w-full justify-start"
                      onClick={() => {
                        setAdding(false);
                        setAssigning([p]);
                      }}
                    >
                      {p.name}
                    </Button>
                  </li>
                ))}
              </ul>
            </div>
          )}
          <p className="mt-4 text-xs text-muted">
            Manual setup generates a client key locally. Register its public key on your server before
            enabling connection attempts. To use existing private or preshared keys, import a .conf file.
          </p>
        </Modal>
      )}
      {assigning[0] && (
        <ServerDetails
          key={assigning[0].id}
          profile={assigning[0]}
          onClose={() => setAssigning((ps) => ps.slice(1))}
        />
      )}
      {editor && (
        <Editor
          id={editor.id}
          initial={editor.draft}
          onClose={() => setEditor(null)}
          onSaved={(p) => setAssigning([p])}
        />
      )}
      {managing && (
        <Modal open title={`Manage ${managing.name}`} onClose={() => setManaging(null)}>
          <div className="flex flex-col gap-2">
            <Button
              disabled={busy}
              onClick={() => {
                setAssigning([managing]);
                setManaging(null);
              }}
            >
              Edit location
            </Button>
            <Button
              disabled={busy || managing.id === activeId || unsafe}
              onClick={() => {
                void edit(managing);
                setManaging(null);
              }}
            >
              Edit configuration
            </Button>
            <Button
              disabled={busy}
              onClick={() => {
                void saveSettings({ ...settings, preferred_profile: managing.id });
                setManaging(null);
              }}
            >
              {managing.preferred ? "Preferred server" : "Set preferred"}
            </Button>
            <Button
              variant="ghost"
              disabled={busy || managing.id === activeId || unsafe}
              onClick={() => {
                setDeleting(managing);
                setManaging(null);
              }}
            >
              Delete server
            </Button>
          </div>
        </Modal>
      )}
      <Confirm
        open={!!deleting}
        title={`Delete ${deleting?.name ?? ""}?`}
        body="This removes the profile, location, history and stored keys from this computer. Keep your original configuration if you need to restore it."
        confirmLabel="Delete server"
        danger
        onCancel={() => setDeleting(null)}
        onConfirm={() => {
          const p = deleting;
          setDeleting(null);
          if (p) void perform(() => api.deleteProfile(p.id));
        }}
      />
      {guide && (
        <Modal open title="Set up your first location" onClose={() => setGuide(false)}>
          <ol className="list-decimal space-y-3 pl-5 text-[13px]">
            <li>
              Get a client .conf from a WireGuard server operator. The server must recognize your client
              public key and tunnel address.
            </li>
            <li>
              Import it, choose its country and city, and save. Locations are user-assigned, not geographic
              verification.
            </li>
            <li>
              Select the country and click Connect. Wait for a server response; tunnel activation alone does
              not prove the remote peer answered.
            </li>
            <li>
              Use Change location on Dashboard to select another server. Switching has a traffic gap without a
              kill switch. Protected switching is currently unsupported and refused.
            </li>
          </ol>
          <Button className="mt-5" onClick={() => setGuide(false)}>
            Close guide
          </Button>
        </Modal>
      )}
    </div>
  );
}

function ServerDetails({ profile, onClose }: { profile: ProfileView; onClose: () => void }) {
  const { countries, snapshot, refreshProfiles } = useStore();
  const [name, setName] = useState(profile.name);
  const [update, setUpdate] = useState<ServerUpdate>({
    location: profile.server.location,
    favorite: profile.server.favorite,
    enabled: profile.server.enabled,
    registration_pending: profile.server.registration_pending,
  });
  const [code, setCode] = useState(update.location?.country_code ?? "");
  const [city, setCity] = useState(update.location?.city ?? "");
  const [region, setRegion] = useState(update.location?.region ?? "");
  const [timeZone, setTimeZone] = useState(update.location?.time_zone ?? "");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const countryId = useId();
  const active =
    snapshot && isActive(snapshot.state) && (!snapshot.profile || snapshot.profile.id === profile.id);
  const save = async () => {
    setSaving(true);
    setError("");
    try {
      if (name !== profile.name) {
        const draft = await api.draft(profile.id);
        await api.updateProfile(profile.id, { ...draft, name });
      }
      await api.updateServer(profile.id, {
        ...update,
        location: {
          country_code: code,
          city: city || null,
          region: region || null,
          time_zone: timeZone || null,
        },
      });
      await refreshProfiles();
      onClose();
    } catch (e) {
      setError(asUiError(e).message);
    } finally {
      setSaving(false);
    }
  };
  return (
    <Modal open title={`Server details: ${profile.name}`} onClose={onClose}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
        className="flex flex-col gap-4"
      >
        {active ? (
          <p className="font-semibold">{profile.name} (disconnect to rename)</p>
        ) : (
          <TextField label="Server name" value={name} onChange={setName} />
        )}
        <div>
          <label htmlFor={countryId} className="mb-1 block text-[13px] font-medium">
            Country
          </label>
          <select
            id={countryId}
            required
            value={code}
            onChange={(e) => setCode(e.target.value)}
            className="w-full rounded-md border border-line bg-surface p-2"
          >
            <option value="">Choose a country</option>
            {countries.map((c) => (
              <option key={c.code} value={c.code}>
                {c.name}
              </option>
            ))}
          </select>
          <p className="mt-1 text-xs text-muted">User-assigned. Geographic location is not verified.</p>
        </div>
        <TextField label="City (optional)" value={city} onChange={setCity} />
        <TextField label="Region (optional)" value={region} onChange={setRegion} />
        <TextField
          label="Time zone (optional)"
          value={timeZone}
          onChange={setTimeZone}
          hint="Descriptive metadata only, for example Asia/Singapore."
        />
        <label className="flex items-center gap-2 text-[13px]">
          <input
            type="checkbox"
            checked={update.favorite}
            onChange={(e) => setUpdate({ ...update, favorite: e.target.checked })}
          />
          Favorite server
        </label>
        <label className="flex items-center gap-2 text-[13px]">
          <input
            type="checkbox"
            disabled={!!active}
            checked={update.enabled}
            onChange={(e) => setUpdate({ ...update, enabled: e.target.checked })}
          />
          Enable connection attempts
        </label>
        {profile.server.registration_pending && (
          <div className="rounded-lg border border-line p-3 text-[13px]">
            <p>Register this client public key and its tunnel addresses with your server operator:</p>
            <p className="selectable mt-2 break-all font-mono text-xs">
              {profile.summary.interface_public_key}
            </p>
            <p className="selectable mt-1 font-mono text-xs">{profile.summary.addresses.join(", ")}</p>
            <label className="mt-3 flex items-start gap-2">
              <input
                type="checkbox"
                checked={!update.registration_pending}
                onChange={(e) => setUpdate({ ...update, registration_pending: !e.target.checked })}
              />
              I registered this client identity on the remote server.
            </label>
            <p className="mt-2 text-xs text-muted">
              If you generated a preshared key, use explicit profile export to deliver it securely to the
              operator. Registration confirmation permits an attempt; it does not verify connectivity.
            </p>
          </div>
        )}
        {error && (
          <p role="alert" className="text-[13px] text-danger">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 border-t border-line pt-4">
          <Button disabled={saving} onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={saving || !code}>
            {saving ? "Saving…" : "Save server"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
