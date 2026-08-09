import { describe, expect, it } from "vitest";
import { LINK_VARIABLES } from "./canvasVariables";
import {
  ANIMATED_LINK_VARIABLES,
  animatesVariable,
  animationAppliesHint,
  canvasAnimates,
  genericPulseInputs,
  type PulseKind,
  pulseApplies,
  pulseKindFor,
  pulsePattern,
  pulseSpeed,
  pulseVariableOf,
} from "./linkPulse";
import type { LinkVariable } from "./types";

/**
 * The animation toggle used to sit inert on three of the five link
 * variables, because the pulse was gated on Flow and Velocity by name. The
 * gate was written when those were the only variables the canvas offered;
 * the pulse itself never read the coloured variable at all.
 *
 * The gate is now one function, which matters four times over: it had been
 * written out by hand in the layer, the animation clock, the legend's own
 * list and the sentence a reader sees — and a variable added to one but not
 * the others renders a pulse that never advances, or one no control can
 * switch on.
 */

const ALL_LINK_VARS: readonly LinkVariable[] = LINK_VARIABLES;

describe("which variables animate", () => {
  it("gives every link variable a decision", () => {
    for (const v of ALL_LINK_VARS) {
      expect(["magnitude", "transport", "presence", "none"]).toContain(
        pulseKindFor(v),
      );
    }
  });

  /**
   * The three rates go together because motion and colour rise and fall
   * together on all of them — unit headloss goes roughly as v^1.85, so a
   * fast pulse really does mean a high reading.
   */
  it("animates every rate the same way", () => {
    expect(pulseKindFor("flow")).toBe("magnitude");
    expect(pulseKindFor("velocity")).toBe("magnitude");
    expect(pulseKindFor("headloss")).toBe("magnitude");
  });

  it("animates status as a yes/no", () => {
    expect(pulseKindFor("status")).toBe("presence");
  });

  /**
   * And quality as something carried. Its colour is a property of the
   * water's content, not of its movement, so the two are free to disagree —
   * a pipe can crawl with high chlorine or race with none. On the rate
   * pattern it would have taught the reader that a fast pulse means a high
   * reading, which is true of the three above and false here.
   */
  it("animates quality as something being carried", () => {
    expect(pulseKindFor("quality")).toBe("transport");
    expect(pulseKindFor("quality")).not.toBe(pulseKindFor("flow"));
  });

  /** Every one of them, so the toggle is never offered dead. */
  it("leaves no variable inert", () => {
    expect(ALL_LINK_VARS.filter((v) => !pulseApplies(v))).toEqual([]);
  });

  /** The gate and the kind cannot disagree, or the clock and the layer will. */
  it("applies exactly where a kind is defined", () => {
    for (const v of ALL_LINK_VARS) {
      expect(pulseApplies(v)).toBe(pulseKindFor(v) !== "none");
    }
  });

  it("knows about every variable the canvas offers", () => {
    expect([...LINK_VARIABLES].sort()).toEqual(
      ["flow", "headloss", "quality", "status", "velocity"].sort(),
    );
  });

  /**
   * The legend's own list is what makes the toggle reachable. It was the
   * third hand-written copy of this decision, and the one that was missed
   * when the first two were extended — so headloss and status animated
   * correctly while the control that switches animation on stayed hidden
   * for them.
   */
  it("offers the toggle for exactly what animates", () => {
    expect([...ANIMATED_LINK_VARIABLES].sort()).toEqual(
      ALL_LINK_VARS.filter(pulseApplies).sort(),
    );
    expect(ANIMATED_LINK_VARIABLES).toContain("headloss");
    expect(ANIMATED_LINK_VARIABLES).toContain("status");
    expect(ANIMATED_LINK_VARIABLES).toContain("quality");
  });
});

describe("a rate pulse", () => {
  const kind: PulseKind = "magnitude";

  it("keeps pace with velocity", () => {
    const slow = pulseSpeed(kind, { flow: 1, velocity: 0.3 }, 10);
    const fast = pulseSpeed(kind, { flow: 1, velocity: 1.2 }, 10);
    expect(fast).toBeGreaterThan(slow);
  });

  it("falls back to flow when there is no velocity", () => {
    expect(pulseSpeed(kind, { flow: 5, velocity: null }, 10)).toBeCloseTo(0.5);
  });

  it("never exceeds full rate", () => {
    expect(pulseSpeed(kind, { flow: 1, velocity: 99 }, 10)).toBeLessThanOrEqual(
      1,
    );
  });

  it("runs backwards on reversed flow", () => {
    expect(pulseSpeed(kind, { flow: -5, velocity: 1 }, 10)).toBeLessThan(0);
  });

  /**
   * Unit headloss reads exactly like flow does. The pulse is the same pulse
   * — only the gate changed — so the reading cannot drift between the
   * variable that was always animated and the one that just started.
   */
  it("is the same pulse headloss gets", () => {
    const link = { flow: -3, velocity: 0.9 };
    expect(pulseSpeed(pulseKindFor("headloss"), link, 10)).toBe(
      pulseSpeed(pulseKindFor("flow"), link, 10),
    );
  });
});

describe("a presence pulse", () => {
  const kind: PulseKind = "presence";

  /**
   * The load-bearing assertion. Status is a categorical legend with no scale
   * for motion to borrow, so a pulse that varied with flow would be showing
   * a magnitude nothing on screen explains.
   */
  it("runs at one rate whatever the flow", () => {
    const trickle = pulseSpeed(kind, { flow: 0.001, velocity: 0.01 }, 500);
    const torrent = pulseSpeed(kind, { flow: 480, velocity: 3 }, 500);
    expect(trickle).toBe(torrent);
  });

  it("still says which way", () => {
    const forward = pulseSpeed(kind, { flow: 5, velocity: 1 }, 10);
    const back = pulseSpeed(kind, { flow: -5, velocity: 1 }, 10);
    expect(back).toBe(-forward);
  });

  it("holds a closed link still", () => {
    expect(pulseSpeed(kind, { flow: 0, velocity: 0 }, 10)).toBe(0);
  });

  /**
   * An open pipe standing idle is the thing this animation exists to show,
   * so it has to read as still rather than as barely moving.
   */
  it("holds an idle link still despite solver noise", () => {
    expect(pulseSpeed(kind, { flow: 1e-14, velocity: 0 }, 500)).toBe(0);
  });

  /** A model can report one column and not the other. */
  it("moves on velocity alone", () => {
    expect(pulseSpeed(kind, { flow: null, velocity: 0.4 }, 10)).not.toBe(0);
  });

  it("holds still when the results say nothing", () => {
    expect(pulseSpeed(kind, { flow: null, velocity: null }, 10)).toBe(0);
  });
});

describe("a variable that does not animate", () => {
  it("is still, whatever the link is doing", () => {
    expect(pulseSpeed("none", { flow: 99, velocity: 9 }, 10)).toBe(0);
  });
});

/**
 * Motion says what the water is doing; the pattern says what kind of claim
 * that is. Two variables measuring the same event the same way share a
 * pattern — flow and velocity are locked together by Q = vA, and drawing
 * them differently would imply a difference the physics does not have.
 */
describe("the pattern each kind draws", () => {
  it("draws a rate as a continuous wave", () => {
    expect(pulsePattern("magnitude")).toBe(0);
  });

  it("draws a yes/no as discrete marks", () => {
    expect(pulsePattern("presence")).toBe(1);
  });

  /** The load-bearing one: identical patterns would put the constant rate
   *  back as an apparent magnitude, which is what this split exists to
   *  prevent. */
  it("does not draw them the same way", () => {
    expect(pulsePattern("presence")).not.toBe(pulsePattern("magnitude"));
  });

  it("draws something carried as parcels", () => {
    expect(pulsePattern("transport")).toBe(2);
  });

  it("gives the three rates one pattern between them", () => {
    expect(pulsePattern(pulseKindFor("velocity"))).toBe(
      pulsePattern(pulseKindFor("flow")),
    );
    expect(pulsePattern(pulseKindFor("headloss"))).toBe(
      pulsePattern(pulseKindFor("flow")),
    );
  });

  /**
   * Three claims, three looks. Any two sharing one puts the reader back to
   * inferring the wrong thing from the motion, which is the whole reason
   * this split exists.
   */
  it("draws no two kinds the same way", () => {
    const drawn = ["magnitude", "transport", "presence"] as const;
    const patterns = new Set(drawn.map(pulsePattern));
    expect(patterns.size).toBe(drawn.length);
  });

  it("gives status and quality patterns of their own", () => {
    expect(pulsePattern(pulseKindFor("status"))).not.toBe(
      pulsePattern(pulseKindFor("flow")),
    );
    expect(pulsePattern(pulseKindFor("quality"))).not.toBe(
      pulsePattern(pulseKindFor("flow")),
    );
    expect(pulsePattern(pulseKindFor("quality"))).not.toBe(
      pulsePattern(pulseKindFor("status")),
    );
  });

  /** Never reaches the shader, but must still answer with a pattern rather
   *  than a sentinel someone has to remember to check. */
  it("answers for a variable that does not animate", () => {
    expect(pulsePattern("none")).toBe(0);
  });
});

/**
 * The sentence shown when the toggle does not apply.
 *
 * It was written out by hand — "Animation applies to flow and velocity" —
 * and stayed that way while the layer, the clock and the legend's gate were
 * all extended past it. It is the only copy of the list a user ever reads,
 * which made it the one place the drift was visible and the last place it
 * was fixed.
 */
describe("what the disabled toggle says", () => {
  it("reads as a sentence rather than a list dump", () => {
    // `Intl.ListFormat` under `en`, Oxford comma and all — which is what
    // shipped before this and is not what is being changed here.
    expect(animationAppliesHint(["Flow", "Velocity", "Status"])).toBe(
      "Animation applies to Flow, Velocity, and Status",
    );
  });

  it("handles a single name without inventing a conjunction", () => {
    expect(animationAppliesHint(["Flow"])).toBe("Animation applies to Flow");
  });

  /**
   * The words belong to the engine being looked at. This module knows the
   * water distribution pulse, and taking ids to translate was still too
   * much knowledge — the ids were this engine's, so drainage was told about
   * Unit headloss and Quality.
   */
  it("says only what it was given", () => {
    expect(animationAppliesHint(["Depth"])).not.toContain("headloss");
  });

  /** An engine with nothing to animate should not be promised anything. */
  it("says so when nothing applies", () => {
    expect(animationAppliesHint([])).not.toContain("applies to");
  });
});

/**
 * Parcels travel at the water's speed.
 *
 * Not a stylistic choice: they stand for the engine's volume segments, and
 * the solver moves those at the flow velocity. A constant rate here would
 * be a prettier lie.
 */
describe("a transport pulse", () => {
  it("moves at the same rate the water does", () => {
    const link = { flow: 4, velocity: 0.8 };
    expect(pulseSpeed("transport", link, 10)).toBe(
      pulseSpeed("magnitude", link, 10),
    );
  });

  it("still says which way", () => {
    expect(
      pulseSpeed("transport", { flow: -4, velocity: 0.8 }, 10),
    ).toBeLessThan(0);
  });
});

/**
 * The pulse on an engine that is not water distribution.
 *
 * Two things kept it still on a drainage map. The gate asked this module
 * whether a variable animates, and this module knows only the wds names —
 * an id it had never heard of fell off the end of the switch and came back
 * `undefined`, which every caller read as "yes". And the values came from
 * the water-distribution result channel, which a catalog-keyed engine never
 * fills: exactly one of the two channels is ever populated, and the flow
 * layer was built from the wrong one, so it was never built at all.
 */

describe("animatesVariable", () => {
  it("takes the engine's answer, not this module's", () => {
    // Drainage animates flow and velocity, and nothing else — depth and
    // capacity are states, not rates.
    const drainage = ["flow", "velocity"];
    expect(animatesVariable("flow", drainage)).toBe(true);
    expect(animatesVariable("capacity", drainage)).toBe(false);
    // The wds module would have said "yes" to both: `capacity` is not in
    // its switch, and the fall-through read as animating.
    expect(pulseKindFor("capacity")).toBe("magnitude");
  });

  it("an engine that animates nothing animates nothing", () => {
    expect(animatesVariable("flow", [])).toBe(false);
  });
});

describe("genericPulseInputs", () => {
  it("reads a flow channel as signed rate", () => {
    // Direction lives in the sign, so the value goes to `flow` whole.
    expect(genericPulseInputs("flow", -3)).toEqual({ flow: -3 });
    expect(pulseSpeed("magnitude", genericPulseInputs("flow", -3), 6)).toBe(
      -0.5,
    );
  });

  it("reads a velocity channel as rate plus direction", () => {
    // `pulseSpeed` takes its rate from velocity and its direction from the
    // sign of flow, so a reversed conduit still runs backwards on screen.
    expect(genericPulseInputs("velocity", -1)).toEqual({
      flow: -1,
      velocity: 1,
    });
    expect(
      pulseSpeed("magnitude", genericPulseInputs("velocity", -1), 1),
    ).toBeLessThan(0);
    expect(
      pulseSpeed("magnitude", genericPulseInputs("velocity", 1), 1),
    ).toBeGreaterThan(0);
  });

  it("an unreported element yields nothing to animate", () => {
    // NaN marks an element the results file does not report.
    expect(genericPulseInputs("flow", Number.NaN)).toEqual({});
    expect(genericPulseInputs("flow", undefined)).toEqual({});
  });
});

describe("pulseVariableOf", () => {
  /**
   * The canvas coerces its `linkVar` into the water-distribution union,
   * answering with a *fallback* for any id outside it — so on a drainage
   * project it reads "flow" whatever is selected. Judging the pulse by
   * that made Depth and Capacity animate while the legend, reading the
   * engine's list, correctly said only Flow and Velocity do.
   */
  it("prefers the generic channel's own variable", () => {
    // Selected Capacity: the coerced id has fallen back to "flow".
    expect(pulseVariableOf("capacity", "flow")).toBe("capacity");
    expect(pulseVariableOf("depth", "flow")).toBe("depth");
  });

  it("is what stops a state variable from pulsing", () => {
    const drainage = ["flow", "velocity"];
    const coerced = "flow";
    // What shipped: the fallback said yes for every drainage variable.
    expect(animatesVariable(coerced, drainage)).toBe(true);
    // What the channel says, which is the truth on screen.
    expect(
      animatesVariable(pulseVariableOf("capacity", coerced), drainage),
    ).toBe(false);
    expect(animatesVariable(pulseVariableOf("flow", coerced), drainage)).toBe(
      true,
    );
  });

  it("leaves an engine without a generic channel alone", () => {
    // wds serves the fixed-variable channel, where the two agree.
    expect(pulseVariableOf(undefined, "headloss")).toBe("headloss");
  });
});

describe("canvasAnimates", () => {
  /**
   * Links and nodes share one clock and one layer rebuild, so whether it
   * runs is a question about the canvas, not about either class. Asked
   * about the links alone, a drainage map coloured by node flooding —
   * while its link variable was a state that never pulses — built its
   * rings once at time zero and left them there.
   */
  it("runs for either class on its own", () => {
    expect(canvasAnimates(true, false)).toBe(true);
    // The reported case: nothing pulses along the links, and the rings
    // still need the clock.
    expect(canvasAnimates(false, true)).toBe(true);
  });

  it("stops only when neither is moving", () => {
    expect(canvasAnimates(false, false)).toBe(false);
  });
});
