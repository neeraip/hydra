import { describe, expect, it } from "vitest";
import type { ArchiveScan } from "../../hooks";
import {
  leftBehindSummary,
  rowImportable,
  rowsFromScan,
  selectionsFrom,
  withEngineChosen,
} from "./archiveImport";

/**
 * The archive review's decisions, tested as data. The wizard renders
 * these; nothing here needs a DOM.
 */

function scanWith(
  models: Partial<ArchiveScan["models"][number]>[],
): ArchiveScan {
  return {
    archivePath: "/tmp/models.zip",
    others: [],
    models: models.map((m, i) => ({
      path: `m${i}.inp`,
      stem: `m${i}`,
      engine: "wds",
      candidates: [],
      nodeCount: 1,
      linkCount: 1,
      findingCount: 0,
      repairs: [],
      sidecars: [],
      error: null,
      ...m,
    })),
  };
}

describe("seeding the review table", () => {
  it("includes recognised entries and seeds names from the stem", () => {
    const rows = rowsFromScan(scanWith([{ stem: "bellinge" }, { stem: "  " }]));
    expect(rows[0]).toMatchObject({ name: "bellinge", include: true });
    // A blank stem cannot name a project.
    expect(rows[1].name).toBe("Untitled Project");
  });

  it("excludes ambiguous entries until their engine is chosen", () => {
    const rows = rowsFromScan(
      scanWith([{ engine: null, candidates: ["wds", "uds"] }]),
    );
    expect(rows[0].include).toBe(false);
    expect(rowImportable(rows[0])).toBe(false);

    const chosen = withEngineChosen(rows, "m0.inp", "uds");
    expect(chosen[0]).toMatchObject({ engine: "uds", include: true });
    expect(rowImportable(chosen[0])).toBe(true);
  });

  it("never lets a failed entry become selectable", () => {
    const rows = rowsFromScan(
      scanWith([{ engine: null, error: "no engine recognises this file" }]),
    );
    expect(rows[0].include).toBe(false);
    // Even a (buggy) engine choice cannot revive it.
    const poked = withEngineChosen(rows, "m0.inp", "wds");
    expect(poked[0].include).toBe(false);
    expect(rowImportable(poked[0])).toBe(false);
  });
});

describe("what the create call sends", () => {
  it("sends only included, importable rows, with name fallback", () => {
    const rows = rowsFromScan(
      scanWith([
        { stem: "keep" },
        { stem: "skip" },
        { engine: null, error: "unreadable" },
      ]),
    );
    rows[1].include = false;
    rows[0].name = "   ";
    const selections = selectionsFrom(rows);
    expect(selections).toEqual([
      { path: "m0.inp", name: "Untitled Project", engine: "wds" },
    ]);
  });
});

describe("the left-behind summary", () => {
  it("is silent when nothing was left behind", () => {
    expect(leftBehindSummary([])).toBe("");
  });

  it("names the first few and counts the rest", () => {
    expect(leftBehindSummary(["rain.dat"])).toBe(
      "Not imported (not model files): rain.dat",
    );
    expect(
      leftBehindSummary(["a.dat", "b.dat", "c.txt", "d.txt", "e.txt"]),
    ).toBe("Not imported (not model files): a.dat, b.dat, c.txt and 2 more");
  });
});
