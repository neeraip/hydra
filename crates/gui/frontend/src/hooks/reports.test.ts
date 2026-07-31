import { describe, expect, it } from "vitest";
import {
  addableBlocks,
  builderStateFromTemplate,
  buildTemplateJson,
  moveSection,
  type ReportBlockInfo,
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
