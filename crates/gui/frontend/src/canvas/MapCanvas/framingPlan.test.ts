import { describe, expect, it } from "vitest";
import { type FramingInputs, planFraming } from "./framingPlan";

/**
 * The schematic's framing pass decides between four different things — put
 * the camera away, wait, frame the network, or return to a camera it kept —
 * and until now it decided them in nested conditions inside an effect, which
 * nothing could reach without running the app.
 *
 * Every camera defect this canvas has had lived in there: a blank schematic
 * from cameras cleared on the first pass, a view switch that framed over a
 * restored camera, an arrival that framed when it should not have. None of
 * them were visible to a test.
 *
 * These pin what it decides. The extraction changed no behaviour, so where
 * the current answer is questionable it is pinned as it is and marked.
 */

const NODES = [{ id: "a" }];
const LINKS = [{ id: "p" }];

function inputs(over: Partial<FramingInputs> = {}): FramingInputs {
  return {
    viewMode: "schematic",
    isActive: true,
    topological: false,
    couplingsResolved: true,
    currentSpace: "plan",
    framedFor: { nodes: NODES, links: LINKS },
    nodes: NODES,
    links: LINKS,
    hasSavedCamera: false,
    ...over,
  };
}

describe("leaving the orthographic renderer", () => {
  /** Keeping it is what lets coming back return you to it. */
  it("stashes the camera under the space it belonged to", () => {
    expect(
      planFraming(inputs({ viewMode: "map", currentSpace: "topological" })),
    ).toEqual({ action: "stash", space: "topological" });
  });

  /** Nothing has been drawn, so there is no camera and no space to file it
   *  under. */
  it("has nothing to stash before anything was drawn", () => {
    expect(
      planFraming(inputs({ viewMode: "map", currentSpace: null })),
    ).toEqual({ action: "wait" });
  });
});

describe("waiting", () => {
  it("does nothing while the canvas is off screen", () => {
    expect(planFraming(inputs({ isActive: false })).action).toBe("wait");
  });

  /**
   * Framing an empty layout would record it as framed, and the real one
   * would never get its turn.
   */
  it("does nothing until the couplings decide the topological layout", () => {
    expect(
      planFraming(inputs({ topological: true, couplingsResolved: false }))
        .action,
    ).toBe("wait");
  });

  /** A plan layout does not wait on them: it has its own coordinates. */
  it("frames a plan layout whether or not couplings have arrived", () => {
    expect(
      planFraming(inputs({ topological: false, couplingsResolved: false }))
        .action,
    ).toBe("frame");
  });
});

describe("which space is being drawn", () => {
  it("follows the layout in use", () => {
    expect(planFraming(inputs({ topological: true }))).toMatchObject({
      space: "topological",
    });
    expect(planFraming(inputs({ topological: false }))).toMatchObject({
      space: "plan",
    });
  });

  /**
   * A plan and a topological layout are different coordinate spaces, so a
   * camera carried from one lands nowhere in the other. The one being left
   * is kept before the move.
   */
  it("keeps the camera of the space being left", () => {
    expect(
      planFraming(inputs({ topological: true, currentSpace: "plan" })),
    ).toMatchObject({ stashPrevious: "plan" });
  });

  it("keeps nothing when the space has not changed", () => {
    expect(
      planFraming(inputs({ topological: false, currentSpace: "plan" })),
    ).toMatchObject({ stashPrevious: null });
  });

  it("keeps nothing on the first pass", () => {
    expect(planFraming(inputs({ currentSpace: null }))).toMatchObject({
      stashPrevious: null,
    });
  });
});

describe("whether a kept camera is used", () => {
  /**
   * A kept camera is the record of having been here before, so it answers
   * both "have I framed this yet" and "where was I". Framing on every
   * arrival is what made switching views feel like it kept pressing Fit
   * network.
   */
  it("returns to it when the network is the one it was kept for", () => {
    expect(planFraming(inputs({ hasSavedCamera: true }))).toMatchObject({
      useSaved: true,
      discardSaved: false,
    });
  });

  it("frames when there is none", () => {
    expect(planFraming(inputs({ hasSavedCamera: false }))).toMatchObject({
      useSaved: false,
    });
  });

  /** A camera kept against a different network frames the wrong thing. */
  it("discards it when the nodes change", () => {
    expect(
      planFraming(inputs({ hasSavedCamera: true, nodes: [{ id: "b" }] })),
    ).toMatchObject({ discardSaved: true, useSaved: false });
  });

  it("discards it when the links change", () => {
    expect(
      planFraming(inputs({ hasSavedCamera: true, links: [{ id: "q" }] })),
    ).toMatchObject({ discardSaved: true, useSaved: false });
  });

  /**
   * Pinned as-is, and questionable.
   *
   * With nothing framed yet, `framedFor` is null and the comparison against
   * it is false on both sides — so a first pass counts as "the network
   * changed" and clears every kept camera before it can be used. That is
   * what the code being replaced did, so the extraction keeps it.
   *
   * It is also exactly why a camera restored from storage never survived to
   * be applied: the first framing pass threw it away. Changing it is now a
   * decision with a name and a test in front of it, rather than an
   * expression nobody could reach.
   */
  it("discards a camera on the very first pass, as it always has", () => {
    expect(
      planFraming(inputs({ framedFor: null, hasSavedCamera: true })),
    ).toMatchObject({ discardSaved: true, useSaved: false });
  });
});
