import { describe, expect, it } from "vitest";
import {
  addableBlocks,
  builderStateFromTemplate,
  buildTemplateJson,
  customisedSummary,
  type FormatStore,
  insertionFromPointer,
  insertionToIndex,
  moveSection,
  type ReportBlockInfo,
  readStoredFormat,
  recommendedOrder,
  rowShift,
  sameOrder,
  writeStoredFormat,
} from "./reports";

const CATALOG: ReportBlockInfo[] = [
  {
    id: "wds.run-summary",
    title: "Run Summary",
    summary: "Network size and window",
  },
  {
    id: "wds.mass-balance",
    title: "Mass Balance",
    summary: "Volumetric closure",
  },
  {
    id: "wds.tank-levels",
    title: "Tank Levels",
    summary: "Head of each tank over time",
  },
];

describe("moveSection", () => {
  const list = ["a", "b", "c", "d"];

  it("moves an item down", () => {
    expect(moveSection(list, 0, 2)).toEqual(["b", "c", "a", "d"]);
  });

  it("moves an item up", () => {
    expect(moveSection(list, 3, 1)).toEqual(["a", "d", "b", "c"]);
  });

  it("leaves the list untouched when dropped on itself", () => {
    expect(moveSection(list, 2, 2)).toEqual(list);
  });

  it("ignores an out-of-range drop rather than reordering", () => {
    // A drop outside the list must not silently move something.
    expect(moveSection(list, 0, 9)).toEqual(list);
    expect(moveSection(list, -1, 2)).toEqual(list);
  });

  it("returns a new array, never the original", () => {
    expect(moveSection(list, 0, 1)).not.toBe(list);
  });
});

describe("addableBlocks", () => {
  it("omits blocks already in the report", () => {
    const result = addableBlocks(CATALOG, ["wds.run-summary"], "");
    expect(result.map((b) => b.id)).toEqual([
      "wds.mass-balance",
      "wds.tank-levels",
    ]);
  });

  it("matches on title, case-insensitively", () => {
    expect(addableBlocks(CATALOG, [], "TANK").map((b) => b.id)).toEqual([
      "wds.tank-levels",
    ]);
  });

  it("matches on summary too, so a section is findable by what it contains", () => {
    expect(addableBlocks(CATALOG, [], "closure").map((b) => b.id)).toEqual([
      "wds.mass-balance",
    ]);
  });

  it("returns nothing when the search matches nothing", () => {
    expect(addableBlocks(CATALOG, [], "zzz")).toEqual([]);
  });

  it("keeps catalog order", () => {
    expect(addableBlocks(CATALOG, [], "").map((b) => b.id)).toEqual(
      CATALOG.map((b) => b.id),
    );
  });
});

describe("buildTemplateJson", () => {
  it("writes only the sections in the report, in order", () => {
    const json = JSON.parse(
      buildTemplateJson({
        title: "Report",
        sections: ["wds.tank-levels", "wds.run-summary"],
        headingById: {},
        optionsById: {},
      }),
    );
    expect(json).toEqual({
      version: 1,
      title: "Report",
      blocks: [{ id: "wds.tank-levels" }, { id: "wds.run-summary" }],
    });
  });

  it("omits a blank heading rather than writing an empty title", () => {
    const json = JSON.parse(
      buildTemplateJson({
        title: "Report",
        sections: ["wds.run-summary"],
        headingById: { "wds.run-summary": "   " },
        optionsById: {},
      }),
    );
    expect(json.blocks[0]).toEqual({ id: "wds.run-summary" });
  });

  it("writes a heading override when set", () => {
    const json = JSON.parse(
      buildTemplateJson({
        title: "Report",
        sections: ["wds.run-summary"],
        headingById: { "wds.run-summary": "Overview" },
        optionsById: {},
      }),
    );
    expect(json.blocks[0]).toEqual({
      id: "wds.run-summary",
      title: "Overview",
    });
  });

  it("does not carry options or headings for a removed section", () => {
    // Removing a section keeps its configuration in memory so re-adding
    // restores it — but the template must not mention a section that is not
    // in the document.
    const json = JSON.parse(
      buildTemplateJson({
        title: "Report",
        sections: ["wds.run-summary"],
        headingById: { "wds.mass-balance": "Balance" },
        optionsById: { "wds.mass-balance": { worstCount: 3 } },
      }),
    );
    expect(json.blocks).toEqual([{ id: "wds.run-summary" }]);
  });
});

describe("builderStateFromTemplate", () => {
  it("round-trips a built template", () => {
    const state = {
      title: "My Report",
      sections: ["wds.mass-balance", "wds.run-summary"],
      headingById: { "wds.mass-balance": "Closure" },
      optionsById: { "wds.run-summary": { worstCount: 4 } },
    };
    expect(builderStateFromTemplate(buildTemplateJson(state), CATALOG)).toEqual(
      state,
    );
  });

  it("drops ids the catalog does not know", () => {
    // A template can outlive the block it names; keeping it would put a row
    // in the outline that can never render.
    const json = JSON.stringify({
      version: 1,
      title: "R",
      blocks: [{ id: "wds.run-summary" }, { id: "wds.retired" }],
    });
    expect(builderStateFromTemplate(json, CATALOG)?.sections).toEqual([
      "wds.run-summary",
    ]);
  });

  it("collapses a duplicated id to its first occurrence", () => {
    const json = JSON.stringify({
      version: 1,
      title: "R",
      blocks: [{ id: "wds.run-summary" }, { id: "wds.run-summary" }],
    });
    expect(builderStateFromTemplate(json, CATALOG)?.sections).toEqual([
      "wds.run-summary",
    ]);
  });

  it("reads a template with no blocks as an empty report", () => {
    const json = JSON.stringify({ version: 1, title: "R", blocks: [] });
    expect(builderStateFromTemplate(json, CATALOG)?.sections).toEqual([]);
  });

  it("rejects an unreadable version rather than guessing", () => {
    const json = JSON.stringify({ version: 99, title: "R", blocks: [] });
    expect(builderStateFromTemplate(json, CATALOG)).toBeNull();
  });

  it("rejects malformed JSON", () => {
    expect(builderStateFromTemplate("{not json", CATALOG)).toBeNull();
  });
});

describe("insertionToIndex", () => {
  it("leaves an upward move alone", () => {
    // Dropping row 3 into the gap before row 1 lands at index 1.
    expect(insertionToIndex(3, 1)).toBe(1);
  });

  it("shifts a downward move down by one", () => {
    // Row 0 dropped into the gap before row 2: once it is lifted out, that
    // gap is index 1. Without this the row lands one short of the target.
    expect(insertionToIndex(0, 2)).toBe(1);
  });

  it("maps a drop at the end to the last index", () => {
    expect(insertionToIndex(0, 4)).toBe(3);
  });

  it("is a no-op for the row's own slots", () => {
    expect(insertionToIndex(2, 2)).toBe(2);
    expect(insertionToIndex(2, 3)).toBe(2);
  });
});

describe("insertionFromPointer", () => {
  // Four 20px rows starting at y=0.
  const rows = [
    { top: 0, height: 20 },
    { top: 20, height: 20 },
    { top: 40, height: 20 },
    { top: 60, height: 20 },
  ];

  it("targets the gap before a row while above its midpoint", () => {
    expect(insertionFromPointer(rows, 5)).toBe(0);
    expect(insertionFromPointer(rows, 25)).toBe(1);
  });

  it("targets the gap after a row once past its midpoint", () => {
    expect(insertionFromPointer(rows, 15)).toBe(1);
    expect(insertionFromPointer(rows, 55)).toBe(3);
  });

  it("returns the end slot below the last row", () => {
    expect(insertionFromPointer(rows, 999)).toBe(4);
  });

  it("clamps to the first slot above the list", () => {
    expect(insertionFromPointer(rows, -50)).toBe(0);
  });

  it("has no slots for an empty list", () => {
    expect(insertionFromPointer([], 10)).toBe(0);
  });
});

describe("customisedSummary", () => {
  const descriptors = [
    {
      key: "worstCount",
      label: "Rows in the worst-performing table",
      help: "",
      kind: { type: "integer", default: 10, min: 1, max: null },
      unit: null,
    },
  ] as const;

  it("reports nothing for a section at its defaults", () => {
    expect(customisedSummary(descriptors, undefined, "")).toEqual([]);
    expect(customisedSummary(descriptors, {}, "   ")).toEqual([]);
  });

  it("names a changed option by its label, not its key", () => {
    expect(customisedSummary(descriptors, { worstCount: 20 }, "")).toEqual([
      "Rows in the worst-performing table",
    ]);
  });

  it("counts a heading override as a customisation", () => {
    expect(customisedSummary(descriptors, undefined, "Overview")).toEqual([
      "Heading",
    ]);
  });

  it("falls back to the raw key for an option it cannot describe", () => {
    // Hand-authored, or from a newer engine — still a customisation, and
    // claiming the section is untouched would be worse than a raw key.
    expect(customisedSummary(descriptors, { mystery: 1 }, "")).toEqual([
      "mystery",
    ]);
  });

  it("lists the heading first, then the options", () => {
    expect(
      customisedSummary(descriptors, { worstCount: 3 }, "Overview"),
    ).toEqual(["Heading", "Rows in the worst-performing table"]);
  });
});

describe("recommendedOrder", () => {
  it("puts the report's sections back into catalog order", () => {
    const shuffled = ["wds.tank-levels", "wds.run-summary", "wds.mass-balance"];
    expect(recommendedOrder(CATALOG, shuffled)).toEqual([
      "wds.run-summary",
      "wds.mass-balance",
      "wds.tank-levels",
    ]);
  });

  it("does not add back sections that were removed", () => {
    // Reordering must not undo a deliberate removal.
    expect(recommendedOrder(CATALOG, ["wds.tank-levels"])).toEqual([
      "wds.tank-levels",
    ]);
  });

  it("leaves an already-ordered report untouched", () => {
    const ids = CATALOG.map((b) => b.id);
    expect(recommendedOrder(CATALOG, ids)).toEqual(ids);
  });

  it("sorts unranked ids to the end, keeping their relative order", () => {
    const result = recommendedOrder(CATALOG, ["x", "wds.mass-balance", "y"]);
    expect(result).toEqual(["wds.mass-balance", "x", "y"]);
  });

  it("does not mutate the input", () => {
    const input = ["wds.tank-levels", "wds.run-summary"];
    recommendedOrder(CATALOG, input);
    expect(input).toEqual(["wds.tank-levels", "wds.run-summary"]);
  });
});

describe("sameOrder", () => {
  it("is true for identical lists", () => {
    expect(sameOrder(["a", "b"], ["a", "b"])).toBe(true);
  });

  it("is false when the order differs", () => {
    expect(sameOrder(["a", "b"], ["b", "a"])).toBe(false);
  });

  it("is false when the lengths differ", () => {
    expect(sameOrder(["a"], ["a", "b"])).toBe(false);
  });

  it("is true for two empty lists", () => {
    expect(sameOrder([], [])).toBe(true);
  });
});

describe("rowShift", () => {
  const SLOT = 32;

  it("moves rows the dragged row passes on the way down up one slot", () => {
    // 0 dragged to 2: rows 1 and 2 step up, row 3 is untouched.
    expect(rowShift(1, 0, 2, SLOT)).toBe(-SLOT);
    expect(rowShift(2, 0, 2, SLOT)).toBe(-SLOT);
    expect(rowShift(3, 0, 2, SLOT)).toBe(0);
  });

  it("moves rows it passes on the way up down one slot", () => {
    // 3 dragged to 1: rows 1 and 2 step down, row 0 is untouched.
    expect(rowShift(1, 3, 1, SLOT)).toBe(SLOT);
    expect(rowShift(2, 3, 1, SLOT)).toBe(SLOT);
    expect(rowShift(0, 3, 1, SLOT)).toBe(0);
  });

  it("never displaces the dragged row itself", () => {
    // It keeps its slot and renders invisible, so the freed space is real.
    expect(rowShift(2, 2, 0, SLOT)).toBe(0);
    expect(rowShift(2, 2, 5, SLOT)).toBe(0);
  });

  it("displaces nothing when the row is dropped back where it started", () => {
    for (const i of [0, 1, 2, 3]) {
      expect(rowShift(i, 2, 2, SLOT)).toBe(0);
    }
  });

  it("shifts by the dragged row's slot, not the passed row's height", () => {
    // Rows are wildly uneven once a settings panel is open; the space freed
    // is always the LIFTED row's, so one displacement fits every row.
    expect(rowShift(1, 0, 1, 200)).toBe(-200);
  });
});

describe("remembered preview format", () => {
  function fakeStore(seed: Record<string, string> = {}): FormatStore & {
    map: Map<string, string>;
  } {
    const map = new Map(Object.entries(seed));
    return {
      map,
      getItem: (k) => map.get(k) ?? null,
      setItem: (k, v) => {
        map.set(k, v);
      },
    };
  }

  it("falls back when the project has no stored format", () => {
    expect(readStoredFormat("p1", "html", fakeStore())).toBe("html");
  });

  it("round-trips a chosen format", () => {
    const store = fakeStore();
    writeStoredFormat("p1", "csv", store);
    expect(readStoredFormat("p1", "html", store)).toBe("csv");
  });

  it("keeps projects apart", () => {
    // The format belongs to the report you are producing, so two projects
    // must not share one.
    const store = fakeStore();
    writeStoredFormat("p1", "csv", store);
    writeStoredFormat("p2", "pdf", store);
    expect(readStoredFormat("p1", "html", store)).toBe("csv");
    expect(readStoredFormat("p2", "html", store)).toBe("pdf");
  });

  it("ignores a stored value this build does not offer", () => {
    // How a format retired between releases stops resolving, instead of
    // selecting a tab that no longer exists.
    const store = fakeStore({ "hydra2-report-format:p1": "xlsx" });
    expect(readStoredFormat("p1", "html", store)).toBe("html");
  });

  it("degrades to the fallback with no storage at all", () => {
    expect(readStoredFormat("p1", "txt", undefined)).toBe("txt");
    expect(() => writeStoredFormat("p1", "txt", undefined)).not.toThrow();
  });

  it("does not throw when storage refuses to write", () => {
    // Private browsing and a full quota both throw on setItem; losing a
    // preference must not interrupt the report.
    const store: FormatStore = {
      getItem: () => null,
      setItem: () => {
        throw new Error("quota exceeded");
      },
    };
    expect(() => writeStoredFormat("p1", "csv", store)).not.toThrow();
  });
});
