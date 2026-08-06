import { describe, expect, it } from "vitest";
import type { LinkVariable, NodeVariable } from "../../canvas/types";
import type { PeriodResults } from "../../hooks/results";
import {
  LINK_RESULT_FIELDS,
  linkResultsAt,
  NODE_RESULT_FIELDS,
  nodeResultsAt,
} from "./mergeResults";

/**
 * Every variable a period computes has to reach the element carrying it.
 *
 * The defect: the link merge set flow, velocity, status and quality by
 * hand and never set `headloss`, so the inspector's card for it could not
 * appear no matter what the run produced. The canvas looked correct
 * throughout because it does its own, separate merge from the same
 * arrays — which is exactly why nobody caught it from the map.
 *
 * These walk the variable unions rather than a list written here, so a
 * variable added to a union and left out of the merge fails here.
 */

const NODE_VARS: NodeVariable[] = ["pressure", "head", "demand", "quality"];
const LINK_VARS: LinkVariable[] = [
  "flow",
  "velocity",
  "status",
  "headloss",
  "quality",
];

/** A period where every array holds a distinct, recognisable value. */
function period(): PeriodResults {
  const one = (v: number) => Float32Array.from([v]);
  return {
    nodeDemand: one(1),
    nodeHead: one(2),
    nodePressure: one(3),
    nodeQuality: one(4),
    linkFlow: one(5),
    linkVelocity: one(6),
    linkHeadloss: one(7),
    linkStatus: one(8),
    linkQuality: one(9),
  };
}

describe("merging a period onto an element", () => {
  it("gives a link every variable the canvas can select", () => {
    const merged = linkResultsAt(period(), 0);
    for (const variable of LINK_VARS) {
      expect(merged[variable], `${variable} did not reach the link`).not.toBe(
        null,
      );
      expect(merged[variable]).toBeTypeOf("number");
    }
  });

  /** The one that was missing, named so the regression is unmistakable. */
  it("carries head loss, which it used not to", () => {
    expect(linkResultsAt(period(), 0).headloss).toBe(7);
  });

  it("gives a node every variable the canvas can select", () => {
    const merged = nodeResultsAt(period(), 0);
    for (const variable of NODE_VARS) {
      expect(merged[variable], `${variable} did not reach the node`).not.toBe(
        null,
      );
    }
  });

  /** Each variable reads its own array, not a neighbour's. */
  it("reads each variable from its own array", () => {
    const merged = linkResultsAt(period(), 0);
    expect(merged.flow).toBe(5);
    expect(merged.velocity).toBe(6);
    expect(merged.headloss).toBe(7);
    expect(merged.status).toBe(8);
    expect(merged.quality).toBe(9);
  });

  /**
   * Quality arrays are absent entirely when no quality simulation ran.
   * `null` is the honest answer there — a zero would be a value the run
   * never produced, and the card would show it as one.
   */
  it("reports null for a variable the run did not produce", () => {
    const withoutQuality = period();
    withoutQuality.linkQuality = undefined;
    withoutQuality.nodeQuality = undefined;
    expect(linkResultsAt(withoutQuality, 0).quality).toBe(null);
    expect(nodeResultsAt(withoutQuality, 0).quality).toBe(null);
    // And the rest still arrive.
    expect(linkResultsAt(withoutQuality, 0).headloss).toBe(7);
  });

  /** An index past the end is absent, not `undefined` leaking through. */
  it("reports null past the end of the arrays", () => {
    const merged = linkResultsAt(period(), 99);
    for (const variable of LINK_VARS) expect(merged[variable]).toBe(null);
  });

  /**
   * NaN is what a Float32Array holds for a value the engine could not
   * compute, and it must not reach a card as a number — `NaN.toFixed(2)`
   * renders the string "NaN".
   */
  it("treats a non-finite reading as absent", () => {
    const broken = period();
    broken.linkHeadloss = Float32Array.from([Number.NaN]);
    expect(linkResultsAt(broken, 0).headloss).toBe(null);
  });

  /** The tables are what make the unions authoritative. */
  it("maps every variable to a field, with no duplicates", () => {
    expect(Object.keys(LINK_RESULT_FIELDS).sort()).toEqual(
      [...LINK_VARS].sort(),
    );
    expect(Object.keys(NODE_RESULT_FIELDS).sort()).toEqual(
      [...NODE_VARS].sort(),
    );
    const fields = Object.values(LINK_RESULT_FIELDS);
    expect(new Set(fields).size).toBe(fields.length);
  });
});
