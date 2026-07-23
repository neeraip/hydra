/**
 * Tests for the pure row-model mappers added to elementsEditorDerivations —
 * the lazy save-time equivalents of the `use*Rows` hooks in hooks/editors.ts
 * (used by DraftContext.saveAll / previewPatches so the provider no longer
 * keeps a duplicate full row-model copy alive).
 */
import { describe, expect, it } from "vitest";
import type { Link, Node } from "../../../hooks";
import {
  buildPreviewPatches,
  collectAllElementIds,
  junctionRowsFromNodes,
  linkRefRowsFromLinks,
  reservoirRowsFromNodes,
  tankRowsFromNodes,
} from "./elementsEditorDerivations";

const nodes: Node[] = [
  {
    id: "J1",
    type: "junction",
    x: 1.004,
    y: 2.006,
    elevation: 10.123,
    baseDemand: 3.456,
    demand: 4.2,
    pressure: 25.34,
  },
  {
    id: "J2",
    type: "junction",
    x: 0,
    y: 0,
    pressure: null,
    demand: null,
  },
  {
    id: "T1",
    type: "tank",
    x: 5,
    y: 6,
    elevation: 100.006,
    pressure: null,
    demand: null,
    tankMinLevel: 0.123,
    tankMaxLevel: 3.456,
    tankInitialLevel: 1.5,
    tankDiameter: 12.345,
    tankVolumeCurve: null,
  },
  {
    id: "R1",
    type: "reservoir",
    x: 7,
    y: 8,
    elevation: 50.019,
    pressure: null,
    demand: null,
    headPattern: "PAT1",
  },
];

const links: Link[] = [
  {
    id: "P1",
    type: "pipe",
    fromId: "J1",
    toId: "J2",
    velocity: 0.5,
    diameter: 100,
  },
  {
    id: "PU1",
    type: "pump",
    fromId: "R1",
    toId: "J1",
    velocity: 1.2,
    diameter: 0,
  },
  {
    id: "V1",
    type: "valve",
    fromId: "T1",
    toId: "J2",
    velocity: 0,
    diameter: 50,
  },
];

describe("junctionRowsFromNodes", () => {
  it("maps only junctions, mirroring useJunctionRows rounding", () => {
    const rows = junctionRowsFromNodes(nodes);
    expect(rows.map((r) => r.id)).toEqual(["J1", "J2"]);
    expect(rows[0]).toEqual({
      id: "J1",
      elevation: 10.12,
      baseDemand: 3.46,
      demand: 4.2,
      pressure: 25.3,
      x: 1,
      y: 2.01,
      belowThreshold: false,
    });
  });

  it("defaults missing fields and keeps null pressure", () => {
    const rows = junctionRowsFromNodes(nodes);
    expect(rows[1]).toEqual({
      id: "J2",
      elevation: 0,
      baseDemand: 0,
      demand: 0,
      pressure: null,
      x: 0,
      y: 0,
      belowThreshold: false,
    });
  });
});

describe("tankRowsFromNodes", () => {
  it("maps only tanks, mirroring useTankRows rounding", () => {
    const rows = tankRowsFromNodes(nodes);
    expect(rows).toEqual([
      {
        id: "T1",
        elevation: 100.01,
        minLevel: 0.12,
        maxLevel: 3.46,
        initialLevel: 1.5,
        diameter: 12.35,
        volumeCurve: null,
        x: 5,
        y: 6,
      },
    ]);
  });
});

describe("reservoirRowsFromNodes", () => {
  it("maps only reservoirs, using elevation as head", () => {
    const rows = reservoirRowsFromNodes(nodes);
    expect(rows).toEqual([
      { id: "R1", head: 50.02, pattern: "PAT1", x: 7, y: 8 },
    ]);
  });
});

describe("linkRefRowsFromLinks", () => {
  it("projects id/from/to for one link type only", () => {
    expect(linkRefRowsFromLinks(links, "pipe")).toEqual([
      { id: "P1", from: "J1", to: "J2" },
    ]);
    expect(linkRefRowsFromLinks(links, "pump")).toEqual([
      { id: "PU1", from: "R1", to: "J1" },
    ]);
    expect(linkRefRowsFromLinks(links, "valve")).toEqual([
      { id: "V1", from: "T1", to: "J2" },
    ]);
  });
});

describe("collectAllElementIds", () => {
  it("collects every node and link id", () => {
    expect(collectAllElementIds(nodes, links)).toEqual(
      new Set(["J1", "J2", "T1", "R1", "P1", "PU1", "V1"]),
    );
  });
});

describe("buildPreviewPatches with projected link rows", () => {
  it("detects rename cascades from linkRefRowsFromLinks projections", () => {
    const items = buildPreviewPatches({
      draftEntries: [{ kind: "junction", id: "J1", field: "id", value: "J9" }],
      pendingAdds: [],
      pendingDeletes: [],
      pendingDeleteKeys: new Set(),
      pipeRowsAll: linkRefRowsFromLinks(links, "pipe"),
      pumpRowsAll: linkRefRowsFromLinks(links, "pump"),
      valveRowsAll: linkRefRowsFromLinks(links, "valve"),
      tempIdPrefix: "__new__:",
    });
    const cascades = items.filter((i) => i.field.endsWith("(cascade)"));
    expect(cascades).toEqual([
      { kind: "pipe", id: "P1", field: "from (cascade)", value: "J9" },
      { kind: "pump", id: "PU1", field: "to (cascade)", value: "J9" },
    ]);
  });

  it("detects node-delete cascades against projected link rows", () => {
    const items = buildPreviewPatches({
      draftEntries: [],
      pendingAdds: [],
      pendingDeletes: [{ kind: "junction", id: "J2" }],
      pendingDeleteKeys: new Set(["junction:J2"]),
      pipeRowsAll: linkRefRowsFromLinks(links, "pipe"),
      pumpRowsAll: linkRefRowsFromLinks(links, "pump"),
      valveRowsAll: linkRefRowsFromLinks(links, "valve"),
      tempIdPrefix: "__new__:",
    });
    const cascadeIds = items
      .filter((i) => i.field === "delete (cascade)")
      .map((i) => i.id);
    expect(cascadeIds).toEqual(["P1", "V1"]);
  });
});
