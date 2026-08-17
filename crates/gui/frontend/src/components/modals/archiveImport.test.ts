import { describe, expect, it } from "vitest";
import type { ArchiveScan } from "../../hooks";
import {
  leftBehindSummary,
  rowImportable,
  rowsFromScan,
  selectionsFrom,
  sidecarNote,
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
    // It states what these entries are, and defers "imported or not" to
    // the per-model rows — which now do carry a model's data files.
    expect(leftBehindSummary(["rain.dat"])).toContain("rain.dat");
    expect(leftBehindSummary(["rain.dat"])).not.toContain("Not imported");
    expect(
      leftBehindSummary(["a.dat", "b.dat", "c.txt", "d.txt", "e.txt"]),
    ).toContain("and 2 more");
  });
});

describe("the sidecar note", () => {
  it("is silent for a model with no external references", () => {
    expect(sidecarNote([])).toBeNull();
  });

  it("reports carried references as travelling with the project", () => {
    const note = sidecarNote([
      {
        file: "rain.dat",
        label: 'rain file "rain.dat"',
        carried: true,
        supported: true,
      },
    ]);
    expect(note?.tone).toBe("ok");
    expect(note?.text).toContain("Imports");
    expect(note?.text).toContain('rain file "rain.dat"');
  });

  it("warns about the missing ones, naming only them", () => {
    const note = sidecarNote([
      {
        file: "rain.dat",
        label: 'rain file "rain.dat"',
        carried: true,
        supported: true,
      },
      {
        file: "hot.hsf",
        label: 'hotstart file "hot.hsf"',
        carried: false,
        supported: true,
      },
    ]);
    expect(note?.tone).toBe("warn");
    expect(note?.text).toContain('hotstart file "hot.hsf"');
    expect(note?.text).not.toContain("rain.dat");
  });
});

describe("unsupported references", () => {
  it("outrank carried: a capability gap is never shown green", () => {
    const note = sidecarNote([
      {
        file: "flows.txt",
        label: 'rainfall interface file "flows.txt"',
        carried: true,
        supported: false,
      },
    ]);
    expect(note?.tone).toBe("warn");
    expect(note?.text).toContain("Not supported yet");
  });
});
