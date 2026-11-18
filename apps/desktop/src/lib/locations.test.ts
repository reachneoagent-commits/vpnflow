import { describe, expect, it } from "vitest";
import { hasRecentResponse, locationGroups } from "./locations";
import type { ProfileView, ServerHealth, Settings } from "./types";

// Simulated directory only: documentation endpoints, no real credentials.
const profile = (id: string, code: string | null): ProfileView => ({
  id,
  name: `Simulated ${id}`,
  created_ms: 0,
  updated_ms: 0,
  preferred: false,
  active: false,
  server: {
    location: code ? { country_code: code, city: "Test city", region: null, time_zone: null } : null,
    enabled: true,
    favorite: false,
    registration_pending: false,
    last_attempt_ms: null,
    last_connected_ms: null,
    last_failure_ms: null,
    last_response_ms: null,
  },
  summary: {
    interface_public_key: "public",
    addresses: ["10.0.0.2/32"],
    dns: [],
    endpoints: ["192.0.2.1:51820"],
    allowed_ips: ["10.0.0.0/24"],
    peer_count: 1,
    route_kind: "split",
    mtu: null,
    has_preshared_key: false,
    warnings: [],
  },
});
const countries = [
  { code: "SG", name: "Singapore" },
  { code: "DE", name: "Germany" },
];
const settings: Settings = {
  theme: "system",
  close_to_tray: true,
  launch_on_login: false,
  connect_on_launch: false,
  preferred_profile: null,
  favorite_countries: [],
  quick_connect_country: null,
  auto_reconnect: true,
  reconnect_max_attempts: 8,
  kill_switch: false,
  kill_switch_allow_lan: false,
  notifications: false,
  connectivity_check: true,
  check_updates: false,
  onboarded: true,
};
const health = (id: string, observed_ms: number): ServerHealth => ({
  id,
  observed_ms,
  reachability: "responded",
  measured_rtt_ms: null,
  last_handshake_ms: null,
});

describe("location directory (simulated)", () => {
  it("groups arbitrary configured countries, preserves unassigned profiles and sorts names", () => {
    const profiles = [profile("1", "SG"), profile("2", "DE"), profile("3", "SG"), profile("4", null)];
    const groups = locationGroups(profiles, countries, settings, [], "", "all", "country", 1000);
    expect(groups.map((g) => [g.name, g.servers.length])).toEqual([
      ["Germany", 1],
      ["Singapore", 2],
      ["Unassigned location", 1],
    ]);
    expect(
      locationGroups(profiles, countries, settings, [], "192.0.2.1", "all", "country", 1000),
    ).toHaveLength(3);
    expect(
      locationGroups(profiles, countries, settings, [], "DE", "all", "country", 1000)[0]?.servers,
    ).toHaveLength(1);
  });

  it("persists the meaning of country and individual favorites in filters", () => {
    const a = profile("1", "SG");
    const b = profile("2", "DE");
    a.server.favorite = true;
    const groups = locationGroups(
      [a, b],
      countries,
      { ...settings, favorite_countries: ["DE"] },
      [],
      "",
      "favorites",
      "favorites",
      1000,
    );
    expect(groups).toHaveLength(2);
    expect(groups[0]?.code).toBe("DE");
  });

  it("recently used means successful connection, never just an attempt", () => {
    const a = profile("1", "SG");
    const b = profile("2", "DE");
    a.server.last_attempt_ms = 1000;
    b.server.last_connected_ms = 500;
    const groups = locationGroups([a, b], countries, settings, [], "", "recent", "recent", 1000);
    expect(groups.map((g) => g.code)).toEqual(["DE"]);
  });

  it("expires observations and excludes disabled and pending servers from available", () => {
    const a = profile("1", "SG");
    const b = profile("2", "DE");
    b.server.registration_pending = true;
    const samples = [health(a.id, 1000), health(b.id, 1000)];
    expect(
      locationGroups([a, b], countries, settings, samples, "", "available", "country", 2000),
    ).toHaveLength(1);
    a.server.enabled = false;
    expect(
      locationGroups([a, b], countries, settings, samples, "", "available", "country", 2000),
    ).toHaveLength(0);
    expect(hasRecentResponse(samples[0], 301001)).toBe(false);
    expect(hasRecentResponse(samples[0], 999)).toBe(false);
  });
});
