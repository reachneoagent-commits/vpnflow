import { describe, expect, it } from "vitest";
import { checkSite } from "./sites";

const host = (s: string, list: string[] = []) => {
  const r = checkSite(s, list);
  return r.ok ? r.host : null;
};
const error = (s: string, list: string[] = []) => {
  const r = checkSite(s, list);
  return r.ok ? null : r.error;
};

describe("checkSite", () => {
  it("accepts names with or without www and pasted links", () => {
    expect(host("metrobyt-mobile.com")).toBe("metrobyt-mobile.com");
    expect(host("www.metrobyt-mobile.com")).toBe("metrobyt-mobile.com");
    expect(host("https://www.metrobyt-mobile.com/")).toBe("metrobyt-mobile.com");
    expect(host(" Example.COM ")).toBe("example.com");
    expect(host("http://shop.example.co.uk:8443/a?b#c")).toBe("shop.example.co.uk");
    expect(host("example.com.")).toBe("example.com");
  });

  it("explains what is wrong", () => {
    expect(error("")).toMatch(/Type a website name/);
    expect(error("metrobyt-mobile")).toMatch(/Try metrobyt-mobile\.com/);
    expect(error("www.")).toMatch(/ending/);
    expect(error("a..b")).toMatch(/ending/);
    expect(error("exa mple.com")).toMatch(/no spaces/);
    expect(error("ex_ample.com")).toMatch(/letters, numbers/);
    expect(error("bücher.example")).toMatch(/letters, numbers/);
    expect(error("user@example.com")).toMatch(/email/);
    expect(error("192.168.1.1")).toMatch(/number address/);
    expect(error("-a.com")).toMatch(/not a valid/);
    expect(error("www.example.com", ["example.com"])).toMatch(/already on the list/);
    expect(
      error(
        "new.com",
        Array.from({ length: 32 }, (_, i) => `s${i}.com`),
      ),
    ).toMatch(/full/);
  });
});
