import { describe, expect, it } from "vitest";
import { activeElement, activeKey, isActiveRow } from "./activeElement";

/** A network where a junction and a pipe are both called "2" — legal in
 *  EPANET, which keeps node and link namespaces separate, and the case
 *  that exposed the bug. */
const JUNCTION_2 = { cls: "point", id: "2" } as const;
const PIPE_2 = { cls: "polyline", id: "2" } as const;

describe("isActiveRow", () => {
  /**
   * The defect: the list compared rows against a bare id, so selecting a
   * junction highlighted every element in the model that happened to share
   * its name.
   */
  it("does not match an element of another class with the same id", () => {
    const active = activeElement("2", null, null);
    expect(isActiveRow(JUNCTION_2, active)).toBe(true);
    expect(isActiveRow(PIPE_2, active)).toBe(false);
  });

  it("matches the other way round too", () => {
    const active = activeElement(null, "2", null);
    expect(isActiveRow(PIPE_2, active)).toBe(true);
    expect(isActiveRow(JUNCTION_2, active)).toBe(false);
  });

  it("matches an areal element only against a region selection", () => {
    const region = { cls: "region", id: "2" } as const;
    expect(isActiveRow(region, activeElement(null, null, "2"))).toBe(true);
    expect(isActiveRow(region, activeElement("2", null, null))).toBe(false);
  });

  it("matches nothing when nothing is selected", () => {
    expect(isActiveRow(JUNCTION_2, null)).toBe(false);
    expect(isActiveRow(JUNCTION_2, activeElement(null, null, null))).toBe(
      false,
    );
  });
});

describe("activeElement", () => {
  it("carries the class of whichever selection is set", () => {
    expect(activeElement("J1", null, null)).toEqual({
      cls: "point",
      id: "J1",
    });
    expect(activeElement(null, "P1", null)).toEqual({
      cls: "polyline",
      id: "P1",
    });
    expect(activeElement(null, null, "S1")).toEqual({
      cls: "region",
      id: "S1",
    });
  });

  it("is null when no selection is set", () => {
    expect(activeElement(null, null, null)).toBeNull();
    expect(activeElement(undefined, undefined, undefined)).toBeNull();
  });
});

describe("activeKey", () => {
  /**
   * The second half of the same bug. The scroll-into-view remembered where
   * it had last scrolled by id, so moving the selection from junction "2"
   * to pipe "2" looked like it had already been handled and the list
   * stayed put — or worse, had scrolled to the wrong row to begin with.
   */
  it("changes when the class changes but the id does not", () => {
    const a = activeKey(activeElement("2", null, null));
    const b = activeKey(activeElement(null, "2", null));
    expect(a).not.toBe(b);
  });

  it("is null when nothing is selected", () => {
    expect(activeKey(null)).toBeNull();
  });

  /** Ids are user-chosen, so the class and id must not be able to run
   * together into another pair's key. */
  it("cannot collide across different class/id pairs", () => {
    const keys = [
      activeKey({ cls: "point", id: "line1" }),
      activeKey({ cls: "polyline", id: "1" }),
      activeKey({ cls: "region", id: "1" }),
      activeKey({ cls: "point", id: "1" }),
    ];
    expect(new Set(keys).size).toBe(keys.length);
  });
});
