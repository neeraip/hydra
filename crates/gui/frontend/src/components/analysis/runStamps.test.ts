import { describe, expect, it } from "vitest";
import {
  durationLabel,
  onOverviewCategory,
  runInstants,
  runStampsLabel,
} from "./runStamps";

describe("runInstants", () => {
  it("returns both instants when both stamps are present", () => {
    const r = runInstants({ startedAtMs: 1_000, finishedAtMs: 2_000 });
    expect(r?.started.getTime()).toBe(1_000);
    expect(r?.finished.getTime()).toBe(2_000);
  });

  it("returns null for absent metadata and for a lone stamp", () => {
    // Results from before the stamps existed serve no fields at all; a
    // file carrying only one is malformed and equally untrustworthy.
    expect(runInstants(null)).toBeNull();
    expect(runInstants(undefined)).toBeNull();
    expect(runInstants({})).toBeNull();
    expect(runInstants({ startedAtMs: 1_000 })).toBeNull();
    expect(runInstants({ finishedAtMs: 2_000 })).toBeNull();
  });
});

describe("onOverviewCategory", () => {
  const cats = ["Summary", "Hydrology", "Network"];

  it("is true only on the first category when tabs exist", () => {
    expect(onOverviewCategory("Summary", cats)).toBe(true);
    expect(onOverviewCategory("Hydrology", cats)).toBe(false);
    expect(onOverviewCategory("Network", cats)).toBe(false);
  });

  it("is true when there are no tabs to choose between", () => {
    expect(onOverviewCategory(null, [])).toBe(true);
    expect(onOverviewCategory("Only", ["Only"])).toBe(true);
  });
});

describe("runStampsLabel", () => {
  it("collapses the finish to a time when the run stays in one day", () => {
    const s = new Date(2026, 7, 23, 14, 5, 12);
    const f = new Date(2026, 7, 23, 14, 5, 34);
    const label = runStampsLabel(s, f, "en-GB");
    expect(label).toBe(
      "Ran 23 Aug 2026, 14:05:12 · finished 14:05:34 · took 22 s",
    );
  });

  it("repeats the date when the run crosses midnight", () => {
    const s = new Date(2026, 7, 23, 23, 59, 0);
    const f = new Date(2026, 7, 24, 0, 3, 0);
    const label = runStampsLabel(s, f, "en-GB");
    expect(label).toBe(
      "Ran 23 Aug 2026, 23:59:00 · finished 24 Aug 2026, 00:03:00 · took 4 min",
    );
  });
});

describe("durationLabel", () => {
  it("scales its unit to the run", () => {
    expect(durationLabel(400)).toBe("0.4 s");
    expect(durationLabel(2_340)).toBe("2.3 s");
    expect(durationLabel(22_400)).toBe("22 s");
    expect(durationLabel(60_000)).toBe("1 min");
    expect(durationLabel(222_000)).toBe("3 min 42 s");
    expect(durationLabel(3_600_000)).toBe("1 h");
    expect(durationLabel(7_500_000)).toBe("2 h 5 min");
  });

  it("clamps a backwards clock to instant rather than negative", () => {
    expect(durationLabel(-5_000)).toBe("0 s");
  });
});
