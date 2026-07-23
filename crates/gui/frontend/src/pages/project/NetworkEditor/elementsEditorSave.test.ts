/**
 * Tests for the pure batch-construction step of the draft save
 * (`buildFieldPatches`): all staged field edits collapse into one
 * `patch_elements` batch — one IPC round trip and one `network-changed`
 * event — instead of one command per changed field.
 */
import { describe, expect, it } from "vitest";
import type { DraftEntry } from "./elementsEditorDerivations";
import { buildFieldPatches } from "./elementsEditorSave";

const TEMP = "__new__";

function build(
  draftEntries: DraftEntry[],
  overrides: {
    pendingDeleteKeys?: Set<string>;
    tempToRealId?: Map<string, string>;
  } = {},
) {
  return buildFieldPatches({
    draftEntries,
    pendingDeleteKeys: overrides.pendingDeleteKeys ?? new Set(),
    tempIdPrefix: TEMP,
    tempToRealId: overrides.tempToRealId ?? new Map(),
  });
}

describe("buildFieldPatches", () => {
  it("returns an empty batch for empty drafts", () => {
    expect(build([])).toEqual([]);
  });

  it("emits one patch per changed field on a single element", () => {
    const patches = build([
      { kind: "junction", id: "J1", field: "elevation", value: 12 },
      { kind: "junction", id: "J1", field: "x", value: 100 },
      { kind: "junction", id: "J1", field: "y", value: 200 },
    ]);
    expect(patches).toEqual([
      { kind: "junction", id: "J1", field: "elevation", value: 12 },
      { kind: "junction", id: "J1", field: "x", value: 100 },
      { kind: "junction", id: "J1", field: "y", value: 200 },
    ]);
  });

  it("collects edits across multiple elements into one batch, in entry order", () => {
    const patches = build([
      { kind: "junction", id: "J1", field: "elevation", value: 5 },
      { kind: "pipe", id: "P1", field: "diameter", value: 300 },
      { kind: "tank", id: "T1", field: "maxLevel", value: 6.5 },
    ]);
    expect(patches.map((p) => `${p.kind}:${p.id}:${p.field}`)).toEqual([
      "junction:J1:elevation",
      "pipe:P1:diameter",
      "tank:T1:maxLevel",
    ]);
  });

  it("drops entries for elements staged for deletion", () => {
    const patches = build(
      [
        { kind: "junction", id: "J1", field: "elevation", value: 1 },
        { kind: "junction", id: "J2", field: "elevation", value: 2 },
      ],
      { pendingDeleteKeys: new Set(["junction:J1"]) },
    );
    expect(patches).toEqual([
      { kind: "junction", id: "J2", field: "elevation", value: 2 },
    ]);
  });

  it("re-targets temp-id entries to the real id assigned at creation", () => {
    const patches = build(
      [{ kind: "junction", id: `${TEMP}1`, field: "elevation", value: 9 }],
      { tempToRealId: new Map([[`${TEMP}1`, "J7"]]) },
    );
    expect(patches).toEqual([
      { kind: "junction", id: "J7", field: "elevation", value: 9 },
    ]);
  });

  it("drops create-only fields for freshly created elements", () => {
    const tempToRealId = new Map([
      [`${TEMP}1`, "J7"],
      [`${TEMP}2`, "P7"],
    ]);
    const patches = build(
      [
        // Node: `id` is consumed by the create call.
        { kind: "junction", id: `${TEMP}1`, field: "id", value: "J7" },
        { kind: "junction", id: `${TEMP}1`, field: "elevation", value: 3 },
        // Link: `id`, `from`, `to` are consumed by the create call.
        { kind: "pipe", id: `${TEMP}2`, field: "id", value: "P7" },
        { kind: "pipe", id: `${TEMP}2`, field: "from", value: "J1" },
        { kind: "pipe", id: `${TEMP}2`, field: "to", value: "J2" },
        { kind: "pipe", id: `${TEMP}2`, field: "diameter", value: 200 },
      ],
      { tempToRealId },
    );
    expect(patches).toEqual([
      { kind: "junction", id: "J7", field: "elevation", value: 3 },
      { kind: "pipe", id: "P7", field: "diameter", value: 200 },
    ]);
  });

  it("drops id/from/to edits for persisted (non-temp) elements", () => {
    // Renames and endpoint changes of persisted elements are unsupported —
    // the backend has no id/from/to patch handler — so such entries are
    // dropped instead of being emitted into a batch that could only fail.
    const patches = build([
      { kind: "junction", id: "J1", field: "id", value: "J1-renamed" },
      { kind: "pipe", id: "P1", field: "id", value: "P1-renamed" },
      { kind: "pipe", id: "P1", field: "from", value: "J2" },
      { kind: "pipe", id: "P1", field: "to", value: "J3" },
      { kind: "junction", id: "J1", field: "elevation", value: 4 },
    ]);
    expect(patches).toEqual([
      { kind: "junction", id: "J1", field: "elevation", value: 4 },
    ]);
  });

  it("drops temp-id entries whose element failed to create", () => {
    // No tempToRealId mapping → the create call failed; there is no target.
    const patches = build([
      { kind: "junction", id: `${TEMP}1`, field: "elevation", value: 3 },
    ]);
    expect(patches).toEqual([]);
  });

  it("checks pending deletion against the real id for temp elements", () => {
    const patches = build(
      [{ kind: "junction", id: `${TEMP}1`, field: "elevation", value: 3 }],
      {
        tempToRealId: new Map([[`${TEMP}1`, "J7"]]),
        pendingDeleteKeys: new Set(["junction:J7"]),
      },
    );
    expect(patches).toEqual([]);
  });
});
