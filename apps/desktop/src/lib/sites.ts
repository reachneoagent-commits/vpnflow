// Checks a "skip the VPN" site entry as the user types, with the same rules
// as `parse_bypass_site` in aegis-core, so mistakes show up before saving.

export type SiteCheck = { ok: true; host: string } | { ok: false; error: string };

const MAX_SITES = 32;

export function checkSite(entry: string, existing: string[] = []): SiteCheck {
  const t = entry.trim();
  if (!t) return { ok: false, error: "Type a website name, such as example.com." };
  let h = t.toLowerCase();
  const scheme = h.indexOf("://");
  if (scheme >= 0) h = h.slice(scheme + 3);
  h = h.split(/[/?#]/)[0] ?? "";
  if (h.includes("@")) {
    return {
      ok: false,
      error: "That looks like an email address. Type the website name only, such as example.com.",
    };
  }
  h = (h.split(":")[0] ?? "").replace(/\.+$/, "");
  if (h.startsWith("www.")) h = h.slice(4);
  if (/\s/.test(h)) {
    return { ok: false, error: "Website names have no spaces. Type it like example.com." };
  }
  if (/[^a-z0-9.-]/.test(h)) {
    return {
      ok: false,
      error:
        "Use only letters, numbers, dots and dashes, as in example.com. Copy the name from the browser's address bar if unsure.",
    };
  }
  const labels = h.split(".");
  if (/^[0-9.]+$/.test(h)) {
    return {
      ok: false,
      error: 'That is a number address. Add it under "Skip the VPN" (addresses) above instead.',
    };
  }
  if (labels.length < 2 || labels.some((l) => l === "")) {
    const hint = labels.length < 2 && h ? ` Try ${h}.com.` : "";
    return {
      ok: false,
      error: `Add the ending too, such as .com or .co.uk.${hint}`,
    };
  }
  if (h.length > 253 || labels.some((l) => l.length > 63 || l.startsWith("-") || l.endsWith("-"))) {
    return { ok: false, error: "That is not a valid website name. Check the spelling." };
  }
  if (existing.includes(h)) return { ok: false, error: `${h} is already on the list.` };
  if (existing.length >= MAX_SITES) {
    return { ok: false, error: `The list is full (${MAX_SITES} sites). Remove one first.` };
  }
  return { ok: true, host: h };
}
