/** @vitest-environment node */

import { describe, expect, it } from "vitest";

import type { GenericClassKey } from "../../../canvas/GenericLegend";
import { locateTarget } from "./locateTarget";

describe("locateTarget", () => {
  it("sends the two classes the search is indexed by, and no others", () => {
    expect(locateTarget("point")).toBe("node");
    expect(locateTarget("polyline")).toBe("link");
  });

  /**
   * The defect: a class with no array of its own used to fall through
   * to the link arrays, so locating the surface's deepest cell would
   * have flown to a conduit.
   */
  it("refuses a class the search has no array for", () => {
    const noArray: GenericClassKey[] = ["region", "surface"];
    for (const cls of noArray) expect(locateTarget(cls)).toBeNull();
  });
});
