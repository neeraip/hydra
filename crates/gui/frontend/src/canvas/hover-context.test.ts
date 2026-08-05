import { describe, expect, it } from "vitest";
import { type HoverState, nextHoverState } from "./hover-context";

const EMPTY: HoverState = {
  hoveredNodeId: null,
  hoveredLinkId: null,
  hoveredRegionId: null,
};

describe("nextHoverState", () => {
  it("hovering one element clears the others", () => {
    const onLink = nextHoverState(EMPTY, "hoveredLinkId", "C-1");
    const onNode = nextHoverState(onLink, "hoveredNodeId", "J-1");
    expect(onNode).toEqual({ ...EMPTY, hoveredNodeId: "J-1" });
  });

  it("clearing a class that is not hovered leaves the hover alone", () => {
    // deck fires onHover per layer with no ordering guarantee: the node's
    // hover can arrive before the link's null. That null must not cancel it.
    const onNode = nextHoverState(EMPTY, "hoveredNodeId", "J-1");
    const staleLinkNull = nextHoverState(onNode, "hoveredLinkId", null);
    expect(staleLinkNull.hoveredNodeId).toBe("J-1");
    expect(staleLinkNull).toBe(onNode);
  });

  it("clearing the hovered class does clear it", () => {
    const onNode = nextHoverState(EMPTY, "hoveredNodeId", "J-1");
    expect(nextHoverState(onNode, "hoveredNodeId", null).hoveredNodeId).toBe(
      null,
    );
  });

  it("re-hovering the same element returns the identical state", () => {
    // Referential equality is the point: a mousemove that stays on one
    // element must not re-render the canvas.
    const onNode = nextHoverState(EMPTY, "hoveredNodeId", "J-1");
    expect(nextHoverState(onNode, "hoveredNodeId", "J-1")).toBe(onNode);
  });

  it("moving between two elements of the same class swaps them", () => {
    const a = nextHoverState(EMPTY, "hoveredNodeId", "J-1");
    const b = nextHoverState(a, "hoveredNodeId", "J-2");
    expect(b.hoveredNodeId).toBe("J-2");
  });

  it("re-hovering an element that is not alone in the state re-asserts it", () => {
    // Should not happen through the setters, but if two classes are ever set
    // at once the next hover must still collapse to one.
    const both: HoverState = {
      ...EMPTY,
      hoveredNodeId: "J-1",
      hoveredLinkId: "C-1",
    };
    expect(nextHoverState(both, "hoveredNodeId", "J-1")).toEqual({
      ...EMPTY,
      hoveredNodeId: "J-1",
    });
  });

  it("clearing an already-empty state returns the identical state", () => {
    expect(nextHoverState(EMPTY, "hoveredNodeId", null)).toBe(EMPTY);
  });
});
