import { describe, expect, it } from "vitest";
import { BASE_CHIP_RADIUS, CHIP_RADIUS } from "./ProjectToolbar";

/**
 * The shape that tells the base model apart from a scenario.
 *
 * This used to be font weight: the base was always bold. Weight also meant
 * "active", so the two collided the moment a scenario was selected, and an
 * inactive base and an inactive scenario differed by one weight step at
 * 11px, which is to say not at all.
 *
 * Shape carries it now, which is why the numbers have bounds rather than
 * values. Anyone is free to retune them; nobody should be free to make the
 * base a pill again, or to square it so far it reads as a button.
 */

/** The radii this app gives its buttons. */
const BUTTON_RADIUS = 6;

describe("the scenario strip's corners", () => {
  it("makes the base model squarer than the scenarios", () => {
    expect(BASE_CHIP_RADIUS).toBeLessThan(CHIP_RADIUS);
  });

  /** A button beside two chips claims to be a different kind of control.
   *  The base is the same choice about a different kind of model. */
  it("keeps it softer than a button", () => {
    expect(BASE_CHIP_RADIUS).toBeGreaterThan(BUTTON_RADIUS);
  });

  /** The chips are pills, which is what makes the contrast legible at all. */
  it("keeps the scenario chips fully round at strip height", () => {
    expect(CHIP_RADIUS).toBeGreaterThanOrEqual(11);
  });
});
