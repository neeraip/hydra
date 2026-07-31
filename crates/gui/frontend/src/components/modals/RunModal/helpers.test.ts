import { describe, expect, it } from "vitest";
import { runnableScenarioIds } from "./helpers";

describe("runnableScenarioIds", () => {
  it("excludes only scenarios with blocking errors", () => {
    // Base is valid, scenario "a" is not: the run still happens, minus "a".
    const counts = new Map<string | null, number>([
      [null, 0],
      ["a", 3],
    ]);
    expect(runnableScenarioIds([null, "a"], counts)).toEqual([null]);
  });

  it("treats an unvalidated scenario as runnable", () => {
    // A missing entry means the check is still in flight — the button must
    // not disable itself while waiting, or opening the modal would flicker.
    expect(runnableScenarioIds(["a"], new Map())).toEqual(["a"]);
  });

  it("returns nothing when every ticked scenario has errors", () => {
    const counts = new Map<string | null, number>([
      [null, 1],
      ["a", 2],
    ]);
    expect(runnableScenarioIds([null, "a"], counts)).toEqual([]);
  });

  it("does not exclude on a zero count, which is how warnings arrive", () => {
    // Only errors are counted into the map; a valid model carrying warnings
    // reaches here as 0 and must run.
    expect(runnableScenarioIds([null], new Map([[null, 0]]))).toEqual([null]);
  });
});
