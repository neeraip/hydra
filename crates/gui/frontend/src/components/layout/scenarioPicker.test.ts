import { describe, expect, it } from "vitest";
import { BASE_MODEL_NAME, scenarioPickerOptions } from "./scenarioPicker";

/**
 * The jump-to-scenario picker listed every scenario and not the base model.
 *
 * The base model is not a scenario — it has no record and no id, it is what
 * `activeScenarioId === null` means — so anything built by listing records
 * leaves it out. The strip beside the picker draws it as a separate chip
 * for that reason, and the picker had no equivalent, which made the one
 * thing every project has the one thing it could not jump to.
 */

const SCENARIOS = [
  { id: "a", name: "Fire flow", state: "simulated" },
  { id: "b", name: "Night demand", state: "not-run" },
];

describe("what the picker offers", () => {
  /** The reported defect. */
  it("includes the base model", () => {
    const ids = scenarioPickerOptions(SCENARIOS, "draft", "").map((o) => o.id);
    expect(ids).toContain(null);
  });

  /** It is the root every scenario descends from. */
  it("puts it first", () => {
    expect(scenarioPickerOptions(SCENARIOS, "draft", "")[0].id).toBeNull();
  });

  it("keeps every scenario, in the order given", () => {
    const opts = scenarioPickerOptions(SCENARIOS, "draft", "");
    expect(opts.map((o) => o.id)).toEqual([null, "a", "b"]);
  });

  /** A row with no name is a blank line. */
  it("names it", () => {
    const base = scenarioPickerOptions(SCENARIOS, "draft", "")[0];
    expect(base.name).toBe(BASE_MODEL_NAME);
  });

  /**
   * The base model has no record, so its state has to be supplied. Without
   * it the row's status dot would be a colour that means nothing.
   */
  it("carries the state it was given", () => {
    expect(scenarioPickerOptions(SCENARIOS, "simulated", "")[0].state).toBe(
      "simulated",
    );
  });

  /** A project can have none, and the base model is still there. */
  it("offers it for a project with no scenarios", () => {
    expect(scenarioPickerOptions([], "draft", "")).toHaveLength(1);
  });
});

describe("filtering", () => {
  it("finds the base model by name", () => {
    const opts = scenarioPickerOptions(SCENARIOS, "draft", "base");
    expect(opts).toHaveLength(1);
    expect(opts[0].id).toBeNull();
  });

  /**
   * And filters it like any other row. Pinning it to the top of every
   * search would put an answer in front of someone who asked a different
   * question.
   */
  it("drops it when the query names something else", () => {
    const opts = scenarioPickerOptions(SCENARIOS, "draft", "fire");
    expect(opts.map((o) => o.id)).toEqual(["a"]);
  });

  it("ignores case and surrounding space", () => {
    expect(
      scenarioPickerOptions(SCENARIOS, "draft", "  BASE ").map((o) => o.id),
    ).toEqual([null]);
  });

  it("can match nothing at all", () => {
    expect(scenarioPickerOptions(SCENARIOS, "draft", "zzz")).toEqual([]);
  });
});
