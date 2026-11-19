import { describe, expect, it } from "vitest";
import { bulkSummary, removeMany } from "./bulk";

describe("removeMany", () => {
  it("keeps going after a failure and reports it", async () => {
    const items = [
      { id: "a", name: "A" },
      { id: "b", name: "B" },
      { id: "c", name: "C" },
    ];
    const r = await removeMany(items, async (id) => {
      if (id === "b") throw { message: "in use" };
    });
    expect(r.done).toBe(2);
    expect(r.failed.map((f) => f.name)).toEqual(["B"]);
    expect(bulkSummary(r)).toContain("Deleted 2 servers");
    expect(bulkSummary(r)).toContain("B");
  });
});
