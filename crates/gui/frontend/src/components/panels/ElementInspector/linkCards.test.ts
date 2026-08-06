import { describe, expect, it } from "vitest";
import type { LinkVariable } from "../../../canvas/types";
import {
  headlossLabel,
  LINK_CARD_ORDER,
  linkCardLabel,
  linkCardQuantity,
  linkCardVariables,
} from "./linkCards";

/**
 * Which cards a link's inspector shows.
 *
 * Two defects motivated this, and they point in opposite directions from
 * the same cause — a hand-written card list that no longer matched the
 * variables. `headloss` was absent, so selecting it on the canvas showed
 * Flow instead; `status` and `quality` were unconditional, so a run with
 * no quality results still showed a Quality box reading "—" for a
 * variable the selector did not offer.
 */

const FULL = {
  flow: 12,
  velocity: 1.4,
  status: 3,
  headloss: 0.8,
  quality: 0.25,
};

describe("the cards a link shows", () => {
  /**
   * The guard against this drifting again: every variable the canvas can
   * select must be renderable. A variable added to the union and not here
   * fails immediately rather than silently falling back to Flow.
   */
  it("can show every variable the canvas offers", () => {
    for (const variable of LINK_CARD_ORDER) {
      const { primary, secondaries } = linkCardVariables(FULL, variable);
      expect(primary, `${variable} cannot be primary`).toBe(variable);
      expect(secondaries).not.toContain(variable);
      expect([...secondaries, primary].sort()).toEqual(
        [...LINK_CARD_ORDER].sort(),
      );
    }
  });

  /** The reported bug, named directly. */
  it("shows head loss when head loss is selected", () => {
    expect(linkCardVariables(FULL, "headloss").primary).toBe("headloss");
  });

  /**
   * The other half: a variable with no value is not a card. This is what
   * stopped a run without water quality from advertising a Quality box.
   */
  it("omits a variable the link has no value for", () => {
    const noQuality = { ...FULL, quality: null };
    const { primary, secondaries } = linkCardVariables(noQuality, "flow");
    expect([primary, ...secondaries]).not.toContain("quality");
  });

  /** Including when that variable is the one selected. */
  it("does not lead with a selected variable that has no value", () => {
    const { primary } = linkCardVariables(
      { ...FULL, headloss: null },
      "headloss",
    );
    expect(primary).not.toBe("headloss");
    expect(primary).toBe("flow");
  });

  /** Status is a code, and zero is a real one — "Closed (XHead)". */
  it("treats a zero status as a value, not as absent", () => {
    const { primary, secondaries } = linkCardVariables(
      { flow: 1, status: 0 },
      "flow",
    );
    expect([primary, ...secondaries]).toContain("status");
  });

  /** A link with nothing to report still renders, rather than crashing. */
  it("falls back to flow when the link reports nothing", () => {
    const { primary, secondaries } = linkCardVariables({}, undefined);
    expect(primary).toBe("flow");
    expect(secondaries).toEqual([]);
  });

  it("leads with the first available when nothing is selected", () => {
    expect(linkCardVariables({ velocity: 2, quality: 1 }, undefined).primary) //
      .toBe("velocity");
  });
});

describe("naming a link's head loss", () => {
  /**
   * A pipe reports head loss per unit length (m/km) and everything else
   * reports it outright (m). One label over two different units would be
   * a quantity error wearing a word.
   */
  it("distinguishes the per-length quantity from the total", () => {
    expect(headlossLabel("pipe")).toBe("Unit Headloss");
    expect(headlossLabel("pump")).toBe("Headloss");
    expect(headlossLabel("valve")).toBe("Headloss");
  });

  it("follows the same split in the quantity it reports", () => {
    expect(linkCardQuantity("headloss", "pipe")).toBe("headloss");
    expect(linkCardQuantity("headloss", "pump")).toBe("length");
  });

  /** Codes and bare concentrations carry no unit to convert through. */
  it("gives status and quality no quantity", () => {
    expect(linkCardQuantity("status", "pipe")).toBeUndefined();
    expect(linkCardQuantity("quality", "pipe")).toBeUndefined();
  });
});

describe("every variable's label", () => {
  it("is non-empty for each of them", () => {
    for (const variable of LINK_CARD_ORDER as LinkVariable[]) {
      expect(linkCardLabel(variable, "pipe").length).toBeGreaterThan(0);
    }
  });
});
