import { describe, expect, it } from "vitest";
import { firstFreeId } from "./ids";

/**
 * One loop, three callers — the canvas's node and link suggestions and
 * the Editor's Add dialog. The copies had drifted where copies drift
 * first, in the exhausted case: two invented a timestamp id, one a
 * count-based id, and all three capped the scan at 9999 for no stated
 * reason.
 */
describe("firstFreeId", () => {
  it("starts at 1 on an empty pool", () => {
    expect(firstFreeId("J", new Set())).toBe("J1");
  });

  it("skips over what is taken", () => {
    expect(firstFreeId("J", new Set(["J1", "J2", "J4"]))).toBe("J3");
  });

  it("counts past ids under other prefixes", () => {
    // A pipe P1 does not block a junction J1.
    expect(firstFreeId("J", new Set(["P1", "P2"]))).toBe("J1");
  });

  it("always finds one, with no cap and no fallback", () => {
    // The pigeonhole the old 9999 cap ignored: among taken.size + 1
    // candidates at least one is free, so a dense pool is walked past
    // rather than answered with a timestamp.
    const dense = new Set(Array.from({ length: 5000 }, (_, i) => `J${i + 1}`));
    expect(firstFreeId("J", dense)).toBe("J5001");
  });

  it("matches exactly, not by prefix", () => {
    expect(firstFreeId("J", new Set(["J10"]))).toBe("J1");
  });
});
