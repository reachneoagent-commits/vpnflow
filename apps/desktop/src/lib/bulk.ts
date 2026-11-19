import { asUiError } from "./api";

export type BulkResult = { done: number; failed: { name: string; message: string }[] };

/** Runs `remove` for each item one at a time, so one failure never stops the rest. */
export async function removeMany<T extends { id: string; name: string }>(
  items: T[],
  remove: (id: string) => Promise<unknown>,
): Promise<BulkResult> {
  const out: BulkResult = { done: 0, failed: [] };
  for (const it of items) {
    try {
      await remove(it.id);
      out.done += 1;
    } catch (e) {
      out.failed.push({ name: it.name, message: asUiError(e).message });
    }
  }
  return out;
}

export function bulkSummary(r: BulkResult): string {
  const ok = `Deleted ${r.done} server${r.done === 1 ? "" : "s"}`;
  if (r.failed.length === 0) return ok;
  return `${ok}. ${r.failed.length} could not be deleted: ${r.failed.map((f) => f.name).join(", ")}.`;
}
