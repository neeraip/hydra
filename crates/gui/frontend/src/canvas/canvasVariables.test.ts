import { describe, expect, it } from "vitest";
import {
  asLinkVariable,
  asNodeVariable,
  LINK_VARIABLES,
  linkVariableFor,
  NODE_VARIABLES,
  nodeVariableFor,
} from "./canvasVariables";

/**
 * The reported defect, twice: the legend's picker showed Quality while the
 * hover chip and the element inspector both showed Velocity.
 *
 * Which variable the canvas is showing had two answers — the legend's
 * selection and a separate pair of typed names that everything else read.
 * A select handler wrote both, so they usually agreed, and every path that
 * wrote only one left them naming different variables. Worse, the split was
 * persisted: once a project's prefs held one of each, reopening it restored
 * the disagreement rather than a wrong-but-consistent state.
 *
 * There is one store now and these derive from it, so the assertion that
 * matters is that the derivation is total — every id the legend can hold
 * comes back as the variable of the same name, never as a fallback.
 */

describe("deriving what the canvas shows from what the legend selected", () => {
  it("gives back the variable the legend is on", () => {
    for (const v of NODE_VARIABLES) {
      expect(nodeVariableFor(v, "pressure")).toBe(v);
    }
    for (const v of LINK_VARIABLES) {
      expect(linkVariableFor(v, "velocity")).toBe(v);
    }
  });

  /**
   * The exact case that was reported. Written out separately from the sweep
   * above because it is the one a reader will come here looking for.
   */
  it("shows quality when quality is selected", () => {
    expect(linkVariableFor("quality", "velocity")).toBe("quality");
    expect(nodeVariableFor("quality", "pressure")).toBe("quality");
  });

  /** Nothing is selected until a catalog has been read. */
  it("falls back on an empty selection", () => {
    expect(nodeVariableFor("", "pressure")).toBe("pressure");
    expect(linkVariableFor("", "velocity")).toBe("velocity");
  });

  /**
   * Another engine's catalog has its own ids, and its canvas paints by a
   * different path. An unrecognised id is not an error.
   */
  it("falls back on another engine's id", () => {
    expect(nodeVariableFor("ponded_depth", "head")).toBe("head");
    expect(linkVariableFor("capacity", "flow")).toBe("flow");
  });

  /** A link variable is not a node variable, whatever it is spelled. */
  it("keeps the two classes apart", () => {
    expect(asNodeVariable("flow")).toBeNull();
    expect(asLinkVariable("pressure")).toBeNull();
    // Except quality, which both classes genuinely have.
    expect(asNodeVariable("quality")).toBe("quality");
    expect(asLinkVariable("quality")).toBe("quality");
  });
});

describe("the canonical variable lists", () => {
  it("name every node variable the canvas offers", () => {
    expect([...NODE_VARIABLES].sort()).toEqual(
      ["demand", "head", "pressure", "quality"].sort(),
    );
  });

  it("name every link variable the canvas offers", () => {
    expect([...LINK_VARIABLES].sort()).toEqual(
      ["flow", "headloss", "quality", "status", "velocity"].sort(),
    );
  });
});
