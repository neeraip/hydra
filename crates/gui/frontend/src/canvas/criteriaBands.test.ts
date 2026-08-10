import { describe, expect, it } from "vitest";
import type { Criterion } from "../components/analysis/criteria";
import { annotationFor, bandsFor, cutsOf, verdictAt } from "./criteriaBands";

/**
 * Reading any engine's criterion as a colour scale.
 *
 * The drainage catalog is used throughout rather than the
 * water-distribution one, because "drainage cannot be scaled by its own
 * criteria" is the bug this replaced.
 */

const velocity: Criterion = {
  key: "velocity",
  label: "Velocity",
  help: "Self-cleansing to erosive.",
  kind: {
    type: "band",
    cuts: [
      { key: "selfCleansing", label: "Self-cleansing", default: 0.6 },
      { key: "erosive", label: "Erosive", default: 3.0 },
    ],
  },
  severities: ["caution", "nominal", "alarm"],
};

const capacity: Criterion = {
  key: "capacity",
  label: "Capacity threshold",
  help: "Treated as full.",
  kind: { type: "value", default: 80 },
  severities: ["nominal", "alarm"],
};

/** Judged in reports, never drawn. */
const freeboardNoSeverities: Criterion = {
  key: "freeboard",
  label: "Freeboard",
  help: "Clearance below the rim.",
  kind: { type: "value", default: 0.3 },
};

const catalog = [velocity, capacity, freeboardNoSeverities];
const banded = (criterion: string) => ({ type: "banded", criterion });

describe("cutsOf", () => {
  it("takes the engine's defaults when nothing is saved", () => {
    expect(cutsOf(velocity, {})).toEqual([0.6, 3.0]);
    expect(cutsOf(capacity, {})).toEqual([80]);
  });

  it("takes what the project saved", () => {
    expect(cutsOf(velocity, { velocity: [0.8, 2.5] })).toEqual([0.8, 2.5]);
    expect(cutsOf(capacity, { capacity: 90 })).toEqual([90]);
  });

  it("falls back per entry rather than trusting a malformed one", () => {
    // A valuation is a file on disk that a user can edit.
    expect(cutsOf(velocity, { velocity: [0.8, Number.NaN] })).toEqual([
      0.8, 3.0,
    ]);
    expect(cutsOf(velocity, { velocity: [0.8] })).toEqual([0.6, 3.0]);
    expect(cutsOf(capacity, { capacity: Number.NaN })).toEqual([80]);
  });
});

describe("bandsFor", () => {
  it("resolves a drainage variable against its own criterion", () => {
    expect(bandsFor(banded("velocity"), catalog, {})).toEqual({
      cuts: [0.6, 3.0],
      severities: ["caution", "nominal", "alarm"],
    });
  });

  it("says nothing for a variable that is not banded", () => {
    expect(bandsFor({ type: "sequential" }, catalog, {})).toBeNull();
    expect(bandsFor({ type: "categorical" }, catalog, {})).toBeNull();
  });

  it("says nothing when the criterion is missing from the catalog", () => {
    // An engine could publish a variable whose criterion is not cataloged;
    // the contract forbids it and the engine tests hold it, but a stale
    // saved view or a mismatched build must not colour by guesswork.
    expect(bandsFor(banded("nonesuch"), catalog, {})).toBeNull();
  });

  it("says nothing for a criterion that states no severities", () => {
    // Judged in reports, never drawn — the numbers exist but nothing says
    // which end of them is bad.
    expect(bandsFor(banded("freeboard"), catalog, {})).toBeNull();
  });

  it("refuses a band the user has left out of order", () => {
    // §7.3 calls this degenerate, and an editor mid-edit produces one.
    // Colouring from backwards cuts would report compliance the numbers do
    // not support.
    expect(
      bandsFor(banded("velocity"), catalog, { velocity: [3.0, 0.6] }),
    ).toBeNull();
  });

  it("refuses a catalog whose severities do not fit its cuts", () => {
    const wrong: Criterion = { ...velocity, severities: ["nominal", "alarm"] };
    expect(bandsFor(banded("velocity"), [wrong], {})).toBeNull();
  });
});

describe("verdictAt", () => {
  const bands = bandsFor(banded("velocity"), catalog, {});
  if (!bands) throw new Error("fixture must resolve");

  it("reads each region", () => {
    expect(verdictAt(0.2, bands)).toBe("caution"); // depositing
    expect(verdictAt(1.5, bands)).toBe("nominal");
    expect(verdictAt(4.0, bands)).toBe("alarm"); // scouring
  });

  it("puts a value exactly on a cut in the region above it", () => {
    // A conduit at exactly the 80% capacity threshold is *at* capacity,
    // not under it — the ends people actually set are where this shows.
    const cap = bandsFor(banded("capacity"), catalog, {});
    if (!cap) throw new Error("fixture must resolve");
    expect(verdictAt(79.9, cap)).toBe("nominal");
    expect(verdictAt(80, cap)).toBe("alarm");
  });
});

describe("annotationFor", () => {
  const bands = bandsFor(banded("velocity"), catalog, {});
  if (!bands) throw new Error("fixture must resolve");

  it("reads in the engine's own vocabulary", () => {
    // Not a phrasing per variable id, which is what confined this to
    // three water-distribution variables.
    expect(annotationFor(velocity, bands, (v) => v.toFixed(1))).toBe(
      "Self-cleansing 0.6 · Erosive 3.0",
    );
  });

  it("names a single-threshold criterion by the criterion itself", () => {
    const cap = bandsFor(banded("capacity"), catalog, {});
    if (!cap) throw new Error("fixture must resolve");
    expect(annotationFor(capacity, cap, (v) => `${v}%`)).toBe(
      "Capacity threshold 80%",
    );
  });
});
