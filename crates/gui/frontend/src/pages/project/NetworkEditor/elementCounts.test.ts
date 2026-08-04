import { describe, expect, it } from "vitest";
import { ELEMENT_KIND_ORDER, elementCounts } from "./elementsEditorDerivations";

const nodes = [{ type: "junction" }, { type: "junction" }, { type: "tank" }];
const links = [{ type: "pipe" }, { type: "pump" }];

describe("elementCounts", () => {
  it("counts the saved model by kind", () => {
    const c = elementCounts(nodes, links, [], []);
    expect(c.junction).toBe(2);
    expect(c.tank).toBe(1);
    expect(c.pipe).toBe(1);
    expect(c.pump).toBe(1);
    expect(c.valve).toBe(0);
  });

  // The rail must agree with the table beside it, and the table shows
  // staged rows — so an unsaved addition has to be counted.
  it("includes staged additions", () => {
    const c = elementCounts(nodes, links, [{ kind: "pump", tempId: "t1" }], []);
    expect(c.pump).toBe(2);
  });

  it("excludes staged deletions", () => {
    const c = elementCounts(nodes, links, [], [{ kind: "junction", id: "J1" }]);
    expect(c.junction).toBe(1);
  });

  it("nets additions against deletions of the same kind", () => {
    const c = elementCounts(
      nodes,
      links,
      [{ kind: "junction", tempId: "t1" }],
      [{ kind: "junction", id: "J1" }],
    );
    expect(c.junction).toBe(2);
  });

  // Staged state can momentarily disagree with the network it was staged
  // against; a negative badge would advertise that rather than absorb it.
  it("never reports a negative count", () => {
    const c = elementCounts([], [], [], [{ kind: "valve", id: "V1" }]);
    expect(c.valve).toBe(0);
  });

  it("reports every kind, including the empty ones", () => {
    const c = elementCounts([], [], [], []);
    for (const k of ELEMENT_KIND_ORDER) expect(c[k]).toBe(0);
  });

  it("ignores element types it does not list", () => {
    const c = elementCounts([{ type: "subcatchment" }], [], [], []);
    expect(Object.values(c).every((n) => n === 0)).toBe(true);
  });
});
