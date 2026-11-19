import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Capabilities, EngineEvent, ProfileView, Settings, Snapshot } from "./lib/types";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const caps = (simulated: boolean, killSwitch = false): Capabilities => ({
  backend: simulated ? "mock" : "network_manager",
  simulated,
  backend_version: null,
  available: { status: "supported" },
  kill_switch: killSwitch
    ? { status: "supported" }
    : { status: "unsupported", detail: "helper not installed" },
  dns_protection: { status: "supported" },
  ipv6_protection: { status: "supported" },
  exact_handshake_time: { status: "unsupported", detail: "needs helper" },
  notes: [],
});

const profile: ProfileView = {
  id: "p1",
  name: "Office",
  created_ms: 0,
  updated_ms: 0,
  preferred: true,
  active: false,
  server: {
    location: null,
    favorite: false,
    enabled: true,
    registration_pending: false,
    last_connected_ms: null,
    last_attempt_ms: null,
    last_failure_ms: null,
    last_response_ms: null,
  },
  summary: {
    interface_public_key: "pub",
    addresses: ["10.0.0.2/32"],
    dns: ["10.0.0.1"],
    endpoints: ["vpn.example.com:51820"],
    allowed_ips: ["0.0.0.0/0", "::/0"],
    route_kind: "full",
    peer_count: 1,
    mtu: null,
    has_preshared_key: false,
    warnings: [],
  },
};

const settings: Settings = {
  theme: "system",
  close_to_tray: true,
  launch_on_login: false,
  connect_on_launch: false,
  preferred_profile: "p1",
  favorite_countries: [],
  quick_connect_country: null,
  auto_reconnect: true,
  reconnect_max_attempts: 8,
  kill_switch: false,
  kill_switch_allow_lan: false,
  bypass: [],
  dns_servers: [],
  trusted_networks_enabled: false,
  trusted_networks: [],
  schedule_enabled: false,
  schedule_connect: "09:00",
  schedule_disconnect: "17:00",
  schedule_days: [1, 2, 3, 4, 5],
  notifications: true,
  connectivity_check: true,
  check_updates: false,
  onboarded: true,
};

const off = { status: "off" } as const;
const snap = (state: Snapshot["state"], p: ProfileView | null = null): Snapshot => ({
  state,
  health: [],
  protection: { kill_switch: off, dns: off, ipv6: off },
  reachability: off,
  public_ip: off,
  stats: null,
  profile: p,
  simulated: false,
});

const mockApi = vi.hoisted(() => ({
  appInfo: vi.fn(),
  capabilities: vi.fn(),
  snapshot: vi.fn(),
  events: vi.fn(),
  profiles: vi.fn(),
  countries: vi.fn(),
  updateServer: vi.fn(),
  serverHealth: vi.fn(),
  refreshServerHealth: vi.fn(),
  quickConnect: vi.fn(),
  settings: vi.fn(),
  setSettings: vi.fn(),
  connect: vi.fn(),
  disconnect: vi.fn(),
  cancel: vi.fn(),
  importDialog: vi.fn(),
  newDraft: vi.fn(),
  draft: vi.fn(),
  createProfile: vi.fn(),
  updateProfile: vi.fn(),
  duplicateProfile: vi.fn(),
  deleteProfile: vi.fn(),
  exportProfile: vi.fn(),
  diagnostics: vi.fn(),
  exportDiagnostics: vi.fn(),
}));

const engineEvents = vi.hoisted(() => ({
  listener: undefined as ((event: EngineEvent) => void) | undefined,
}));

vi.mock("./lib/api", async (orig) => {
  const real = await orig<typeof import("./lib/api")>();
  return {
    ...real,
    api: mockApi,
    onEngineEvent: vi.fn(async (listener: (event: EngineEvent) => void) => {
      engineEvents.listener = listener;
      return () => {
        engineEvents.listener = undefined;
      };
    }),
    onImportResult: vi.fn(async () => () => {}),
  };
});

import { App } from "./App";

function setup(opts: { simulated?: boolean; profiles?: ProfileView[]; snapshot?: Snapshot } = {}) {
  mockApi.appInfo.mockResolvedValue({ version: "0.1.0", os: "linux", arch: "x86_64", tray_available: true });
  mockApi.capabilities.mockResolvedValue(caps(opts.simulated ?? false));
  mockApi.snapshot.mockResolvedValue(opts.snapshot ?? snap({ state: "disconnected" }));
  mockApi.countries.mockResolvedValue([
    { code: "SG", name: "Singapore" },
    { code: "DE", name: "Germany" },
  ]);
  mockApi.profiles.mockResolvedValue(opts.profiles ?? [profile]);
  mockApi.settings.mockResolvedValue(settings);
  mockApi.events.mockResolvedValue([]);
  mockApi.diagnostics.mockResolvedValue([]);
  mockApi.connect.mockResolvedValue(undefined);
  return render(<App />);
}

describe("App", () => {
  beforeEach(() => vi.clearAllMocks());

  it("labels the simulated backend so mock connections never look real", async () => {
    setup({ simulated: true });
    expect(await screen.findByRole("alert")).toHaveTextContent(/not protected/i);
  });

  it("shows no simulated banner for a real backend", async () => {
    setup();
    await screen.findByRole("heading", { name: "Not connected" });
    expect(screen.queryByText(/Simulated backend/)).not.toBeInTheDocument();
  });

  it("does not claim Connected before the server has responded", async () => {
    setup({
      snapshot: snap({ state: "connected", profile: "p1", since_ms: Date.now(), peer: "pending" }, profile),
    });
    const h = await screen.findByRole("heading", { level: 1 });
    expect(h).toHaveTextContent("Tunnel up, waiting for Office");
    expect(h).not.toHaveTextContent(/^Connected/);
    expect(screen.getByText("Waiting for data from the server")).toBeInTheDocument();
  });

  it("lights internet reachability only after the check answered, and shows failures", async () => {
    const connected = snap(
      { state: "connected", profile: "p1", since_ms: Date.now(), peer: "confirmed" },
      profile,
    );
    const tones = async () =>
      within(await screen.findByRole("list", { name: "What is confirmed" }))
        .getAllByRole("listitem")
        .map((li) => li.getAttribute("data-tone"));

    const untested = setup({ snapshot: connected });
    expect(await tones()).toEqual(["ok", "ok", "ok", "untested"]);
    untested.unmount();

    const answered = setup({
      snapshot: { ...connected, reachability: { status: "verified", detail: "answered" } },
    });
    expect(await tones()).toEqual(["ok", "ok", "ok", "ok"]);
    expect(screen.getByText("Tunnel DNS answered")).toBeInTheDocument();
    answered.unmount();

    setup({
      snapshot: {
        ...connected,
        reachability: { status: "failed", detail: "no answer from the tunnel's DNS server" },
      },
    });
    expect(await tones()).toEqual(["ok", "ok", "ok", "bad"]);
    expect(screen.getByText("no answer from the tunnel's DNS server")).toBeInTheDocument();
  });

  it("reports unavailable values explicitly instead of blanks", async () => {
    setup();
    await screen.findByRole("heading", { name: "Not connected" });
    expect(screen.getByText("Not tested")).toBeInTheDocument();
    expect(screen.getByText("Last server response")).toBeInTheDocument();
    expect(screen.getByText("DNS resolver egress IPv4")).toBeInTheDocument();
  });

  it("shows last server response and public IP when connected", async () => {
    const since = Date.now() - 5_000;
    setup({
      snapshot: {
        ...snap({ state: "connected", profile: "p1", since_ms: since, peer: "confirmed" }, profile),
        stats: {
          rx_bytes: 1000,
          tx_bytes: 500,
          rx_rate_bps: 0,
          tx_rate_bps: 0,
          last_handshake_ms: null,
          last_response_ms: since,
          interface: "aegis0",
          sampled_ms: Date.now(),
        },
        public_ip: { status: "verified", detail: "203.0.113.50" },
      },
    });
    await screen.findByRole("heading", { level: 1 });
    expect(screen.getByText("Last server response")).toBeInTheDocument();
    expect(screen.getByText("203.0.113.50")).toBeInTheDocument();
  });

  it.each(["p1", null])("keeps disconnect available with unknown status and profile %s", async (id) => {
    setup({
      snapshot: snap(
        { state: "uncertain", profile: id, reason: "Cannot read tunnel status" },
        id ? profile : null,
      ),
    });
    expect(await screen.findByRole("heading", { name: "Tunnel status unknown" })).toBeInTheDocument();
    expect(screen.getByText("Cannot read tunnel status")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Connect" })).not.toBeInTheDocument();
  });

  it("says when the kill switch is still blocking and offers to stop it", async () => {
    const failed = snap({ state: "failed", profile: "p1", reason: "reconnect attempts exhausted" });
    setup({
      snapshot: {
        ...failed,
        protection: { ...failed.protection, kill_switch: { status: "verified", detail: "blocking" } },
      },
    });
    expect(await screen.findByText(/kill switch is blocking ordinary traffic/i)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Stop blocking" }));
    expect(mockApi.disconnect).toHaveBeenCalled();
  });

  it("explains silence from the server without blaming the config after it worked", async () => {
    setup({
      snapshot: snap(
        { state: "connected", profile: "p1", since_ms: Date.now(), peer: "no_response" },
        profile,
      ),
    });
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("No response from Office");
    expect(screen.getByText(/network may be down or switched/i)).toBeInTheDocument();
  });

  it("connects the selected profile", async () => {
    setup();
    await userEvent.click(await screen.findByRole("button", { name: "Connect" }));
    expect(mockApi.connect).toHaveBeenCalledWith("p1");
  });

  it("invites import when there are no profiles", async () => {
    setup({ profiles: [] });
    expect(await screen.findByRole("button", { name: "Import profile" })).toBeInTheDocument();
  });

  it("navigates by keyboard and explains an unsupported kill switch", async () => {
    setup();
    await screen.findByRole("heading", { name: "Not connected" });
    const nav = screen.getByRole("navigation", { name: "Main" });
    const settingsBtn = within(nav).getByRole("button", { name: "Settings" });
    settingsBtn.focus();
    await userEvent.keyboard("{Enter}");
    expect(settingsBtn).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("switch", { name: "Kill switch" })).toHaveAccessibleDescription(
      /not available on this system.*refuses to connect/i,
    );
  });

  it("requires confirmation before deleting a profile", async () => {
    setup();
    await screen.findByRole("heading", { name: "Not connected" });
    await userEvent.click(
      within(screen.getByRole("navigation")).getByRole("button", { name: "Advanced WireGuard" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Delete Office" }));
    expect(mockApi.deleteProfile).not.toHaveBeenCalled();
    await userEvent.click(await screen.findByRole("button", { name: "Delete profile", hidden: true }));
    await act(async () => {});
    expect(mockApi.deleteProfile).toHaveBeenCalledWith("p1");
  });

  it("warns that export contains the private key", async () => {
    setup();
    await screen.findByRole("heading", { name: "Not connected" });
    await userEvent.click(
      within(screen.getByRole("navigation")).getByRole("button", { name: "Advanced WireGuard" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Export Office" }));
    expect(await screen.findByText(/contains the private key/i)).toBeInTheDocument();
    expect(mockApi.exportProfile).not.toHaveBeenCalled();
  });
});

const located = (code: string, name = "Office", id = "p1"): ProfileView => ({
  ...profile,
  id,
  name,
  server: {
    ...profile.server,
    location: {
      country_code: code,
      city: code === "SG" ? "Singapore city" : "Berlin",
      region: null,
      time_zone: null,
    },
  },
});

async function openLocations() {
  await screen.findByRole("heading", { level: 1 });
  await userEvent.click(
    within(screen.getByRole("navigation", { name: "Main" })).getByRole("button", { name: "Locations" }),
  );
  return screen.findByRole("heading", { name: "VPN Locations" });
}

describe("Locations", () => {
  beforeEach(() => vi.resetAllMocks());

  it("shows an empty directory with import, add and setup guidance", async () => {
    setup({ profiles: [] });
    await openLocations();
    expect(screen.getByRole("heading", { name: "Import a WireGuard file" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import WireGuard Configuration" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "View Setup Guide" }));
    expect(screen.getByText(/server must recognize your client public key/i)).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Country directory" })).not.toBeInTheDocument();
  });

  it("selects, collapses and searches actual configured countries and endpoints", async () => {
    setup({ profiles: [located("SG"), located("DE", "Berlin test", "p2")] });
    await openLocations();
    const directory = screen.getByRole("region", { name: "Country directory" });
    await userEvent.click(within(directory).getByRole("button", { name: /^Singapore/ }));
    expect(screen.getByRole("heading", { name: "Office" })).toBeInTheDocument();
    await userEvent.click(within(directory).getByRole("button", { name: /^Singapore/ }));
    expect(screen.getByText("Select a country")).toBeInTheDocument();
    await userEvent.type(screen.getByRole("searchbox", { name: "Search locations" }), "Berlin");
    expect(within(directory).queryByRole("button", { name: /^Singapore/ })).not.toBeInTheDocument();
    await userEvent.click(within(directory).getByRole("button", { name: /^Germany/ }));
    expect(screen.getByRole("heading", { name: "Berlin test" })).toBeInTheDocument();
    expect(screen.getByText("Reachability unknown")).toBeInTheDocument();
    expect(screen.getByText("Not observed")).toBeInTheDocument();
    expect(screen.queryByText("Not measured")).not.toBeInTheDocument();
  });

  it("connects by the stored profile ID and forwards failures", async () => {
    setup({ profiles: [located("SG")] });
    mockApi.connect.mockRejectedValue({ message: "VPN authorization denied", kind: "permission_denied" });
    await openLocations();
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(mockApi.connect).toHaveBeenCalledWith("p1");
    expect(await screen.findByRole("alert")).toHaveTextContent("VPN authorization denied");
  });

  it("uses favorites and requires a recent response for the available filter", async () => {
    setup({ profiles: [located("SG")] });
    await openLocations();
    await userEvent.click(screen.getByRole("radio", { name: "Favorites" }));
    expect(screen.getByText(/No favorite servers yet/i)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: "All countries" }));
    await userEvent.click(screen.getByRole("button", { name: "Favorite Office" }));
    expect(mockApi.updateServer).toHaveBeenCalledWith("p1", expect.objectContaining({ favorite: true }));
    await userEvent.click(screen.getByRole("radio", { name: "Available servers" }));
    expect(screen.getByText(/No servers have answered recently/i)).toBeInTheDocument();
    expect(
      screen.getByText(/Available only lists servers that returned an authenticated WireGuard response/i),
    ).toBeInTheDocument();
  });

  it("imports configuration then saves country metadata without raw keys", async () => {
    mockApi.importDialog.mockResolvedValue([located("SG")]);
    mockApi.updateServer.mockResolvedValue(located("SG"));
    setup({ profiles: [] });
    await openLocations();
    await userEvent.click(screen.getByRole("button", { name: "Import WireGuard Configuration" }));
    expect(
      await screen.findByRole("heading", { name: "Server details: Office", hidden: true }),
    ).toBeInTheDocument();
    const country = screen.getByRole("combobox", { name: "Country", hidden: true });
    await userEvent.selectOptions(country, "DE");
    await userEvent.click(screen.getByRole("button", { name: "Save server", hidden: true }));
    expect(mockApi.updateServer).toHaveBeenCalledWith(
      "p1",
      expect.objectContaining({ location: expect.objectContaining({ country_code: "DE" }) }),
    );
    expect(screen.queryByLabelText("Client private key")).not.toBeInTheDocument();
  });

  it("highlights the active server and synchronizes Dashboard from Rust events", async () => {
    const p = located("SG");
    setup({ profiles: [p] });
    await openLocations();
    const updated = snap({ state: "connected", profile: p.id, since_ms: Date.now(), peer: "confirmed" }, p);
    await act(async () => engineEvents.listener?.({ type: "snapshot", data: updated }));
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeInTheDocument();
    await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name: "Dashboard" }));
    expect(screen.getByRole("heading", { name: "Connected to Office" })).toBeInTheDocument();
    expect(screen.getByText("Singapore city, Singapore")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Change location" }));
    expect(screen.getByRole("heading", { name: "VPN Locations" })).toBeInTheDocument();
  });

  it("disables conflicting transitions and keeps cancellation accessible", async () => {
    const p = located("SG");
    setup({
      profiles: [p, located("DE", "Germany test", "p2")],
      snapshot: snap({ state: "connecting", profile: p.id }, p),
    });
    await openLocations();
    expect(screen.getByRole("button", { name: "Quick Connect" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(mockApi.cancel).toHaveBeenCalled();
  });

  it("requires confirmation before deleting the server and its credentials", async () => {
    setup({ profiles: [located("SG")] });
    await openLocations();
    await userEvent.click(screen.getByLabelText("Manage Office"));
    expect(await screen.findByRole("heading", { name: "Manage Office", hidden: true })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Delete server", hidden: true }));
    expect(mockApi.deleteProfile).not.toHaveBeenCalled();
    const dialog = screen.getByRole("heading", { name: "Delete Office?", hidden: true }).closest("dialog");
    expect(dialog).not.toBeNull();
    await userEvent.click(
      within(dialog as HTMLElement).getByRole("button", { name: "Delete server", hidden: true }),
    );
    expect(mockApi.deleteProfile).toHaveBeenCalledWith("p1");
  });
});

describe("Locations cancellation and retained protection", () => {
  beforeEach(() => vi.resetAllMocks());

  it("keeps Cancel enabled while its connection command is pending", async () => {
    const p = located("SG");
    setup({ profiles: [p] });
    let finish = () => {};
    mockApi.connect.mockReturnValue(
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
    );
    await openLocations();
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    await act(async () =>
      engineEvents.listener?.({ type: "snapshot", data: snap({ state: "connecting", profile: p.id }, p) }),
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(mockApi.cancel).toHaveBeenCalled();
    await act(async () => finish());
  });

  it("allows explicitly stopping a retained block after a failed attempt", async () => {
    const p = located("SG");
    const snapshot = snap(
      { state: "failed", profile: p.id, reason: "Destination failed; traffic remains blocked" },
      p,
    );
    snapshot.protection.kill_switch = { status: "verified", detail: "Traffic blocked" };
    setup({ profiles: [p], snapshot });
    await openLocations();
    await userEvent.click(screen.getByRole("button", { name: "Stop blocking" }));
    expect(mockApi.disconnect).toHaveBeenCalled();
  });
});

it("updates Settings when the Rust engine changes them", async () => {
  setup();
  await screen.findByRole("heading", { level: 1 });
  await act(async () =>
    engineEvents.listener?.({
      type: "settings_changed",
      data: { ...settings, theme: "dark" },
    }),
  );
  expect(document.documentElement).toHaveAttribute("data-theme", "dark");
});
