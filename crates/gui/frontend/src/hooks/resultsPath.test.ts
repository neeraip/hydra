import { describe, expect, it } from "vitest";
import { type ResultMeta, resultsPath } from "./results";

const meta = (over: Partial<ResultMeta>): ResultMeta =>
  ({
    times: [0, 3600],
    hasPeriodData: true,
    ranges: {} as ResultMeta["ranges"],
    qualityMode: "none",
    ...over,
  }) as ResultMeta;

describe("resultsPath", () => {
  // The regression this exists for: every engine publishes a §6 catalog,
  // and reading its presence as "the values come from the catalog path"
  // sent water-distribution results to a payload nothing serves for them.
  // The canvas then painted every element in the network-at-rest palette —
  // results loaded, whole map grey, no error raised anywhere.
  it("ignores the catalog entirely", () => {
    const catalog = { pointVars: [], polylineVars: [], regionVars: [] };
    expect(resultsPath(meta({ generic: catalog, genericPeriods: false }))).toBe(
      "fixed",
    );
    expect(resultsPath(meta({ generic: catalog, genericPeriods: true }))).toBe(
      "generic",
    );
    // ...and a target with no catalog still serves its fixed arrays.
    expect(resultsPath(meta({ generic: undefined }))).toBe("fixed");
  });

  it("takes the generic payload only when the target says so", () => {
    expect(resultsPath(meta({ genericPeriods: true }))).toBe("generic");
    expect(resultsPath(meta({ genericPeriods: false }))).toBe("fixed");
  });

  // Absent means "an older backend that predates the flag", which served
  // the fixed arrays — so absence must not be read as generic.
  it("defaults to the fixed arrays when the flag is absent", () => {
    expect(resultsPath(meta({}))).toBe("fixed");
  });

  it("reports no data when the target serves no periods", () => {
    expect(resultsPath(meta({ hasPeriodData: false }))).toBe("none");
    // Even if it publishes a catalog and claims the generic encoding.
    expect(
      resultsPath(meta({ hasPeriodData: false, genericPeriods: true })),
    ).toBe("none");
  });

  it("reports no data for an absent meta", () => {
    expect(resultsPath(null)).toBe("none");
    expect(resultsPath(undefined)).toBe("none");
  });
});
