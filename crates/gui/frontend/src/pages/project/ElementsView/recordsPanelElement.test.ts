import { describe, expect, it } from "vitest";
import { recordsPanelElement } from "./recordsPanelElement";

/**
 * The gate that kept every container's records invisible. The backend
 * served a control measure's six layers, the panel could draw them, and
 * `spatial && selectedId` between the two answered "never" for any
 * collection — verified live: selecting the one LID control in a model
 * built for the purpose drew no tables at all.
 */
describe("recordsPanelElement", () => {
  it("follows the canvas selection for a spatial kind", () => {
    expect(recordsPanelElement(true, "J1", null)).toBe("J1");
    expect(recordsPanelElement(true, null, null)).toBeNull();
  });

  it("follows the opened container for a collection", () => {
    // A control measure has no geometry, so no canvas selection ever
    // names it. The Editor's own container selection is the only
    // selection it has.
    expect(recordsPanelElement(false, null, "BC1")).toBe("BC1");
  });

  it("never crosses the two selections", () => {
    // A stale canvas selection must not leak into a container tab, nor
    // an opened container into a spatial one: the two selections name
    // elements of different kinds, and either crossing shows one
    // element's records under another's table.
    expect(recordsPanelElement(false, "J1", null)).toBeNull();
    expect(recordsPanelElement(true, null, "BC1")).toBeNull();
  });
});
