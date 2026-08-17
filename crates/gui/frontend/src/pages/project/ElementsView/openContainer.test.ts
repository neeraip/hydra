import { describe, expect, it } from "vitest";
import { openContainerOn } from "./openContainer";

/**
 * The stale-selection repro, as a rule: a container opened on one tab
 * must not leak into another. It did — a control measure opened on the
 * LID tab kept its layer tables on screen from Curves through Inlet
 * designs, because the selection was a bare id and every collection tab
 * could answer for it.
 */
describe("openContainerOn", () => {
  it("answers on the tab the selection was made on", () => {
    expect(
      openContainerOn({ kind: "lidcontrol", id: "BC1" }, "lidcontrol"),
    ).toBe("BC1");
  });

  it("answers nothing on any other tab", () => {
    expect(
      openContainerOn({ kind: "lidcontrol", id: "BC1" }, "curve"),
    ).toBeNull();
    expect(openContainerOn({ kind: "lidcontrol", id: "BC1" }, null)).toBeNull();
  });

  it("answers nothing when nothing is open", () => {
    expect(openContainerOn(null, "curve")).toBeNull();
  });
});
