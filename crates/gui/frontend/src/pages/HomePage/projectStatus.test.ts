import { describe, expect, it } from "vitest";
import type { Project } from "../../hooks";
import { modelSize, projectStatus } from "./projectStatus";

/**
 * What a home-page row says about a project.
 *
 * The row used to show a name and a date, both true and neither answering
 * the question a reader has on opening the app: whether the results in a
 * project still describe the model in it.
 */

const p = (over: Partial<Project>): Project =>
  ({
    id: "1",
    name: "Town",
    engine: "wds",
    state: "draft",
    scenarioCount: 1,
    modifiedLabel: "2 days ago",
    nodeCount: 0,
    linkCount: 0,
    ...over,
  }) as Project;

describe("a project's status line", () => {
  /**
   * The case the whole thing exists for. A stale project opens showing
   * numbers computed from a network that no longer exists, and the row is
   * the last chance to say so before that happens.
   */
  it("warns that a model was edited after its last run", () => {
    const s = projectStatus(p({ state: "stale" }));
    expect(s.label).toBe("Edited since last run");
    expect(s.tone).toBe("attention");
  });

  it("says when results are from, not just that they exist", () => {
    expect(
      projectStatus(p({ state: "simulated", lastRunLabel: "yesterday" })).label,
    ).toBe("Results from yesterday");
  });

  /** A run may be recorded without a date. The claim still has to be true. */
  it("still reports results when the run has no date", () => {
    expect(projectStatus(p({ state: "simulated" })).label).toBe(
      "Results ready",
    );
  });

  /**
   * A stale project has a last-run date too, and it is the misleading
   * number rather than the useful one. It must not be shown as though the
   * results were current.
   */
  it("does not date a stale project by its last run", () => {
    const s = projectStatus(p({ state: "stale", lastRunLabel: "yesterday" }));
    expect(s.label).not.toContain("yesterday");
  });

  it("reads a run that failed as a failure", () => {
    const s = projectStatus(p({ state: "failed" }));
    expect(s.tone).toBe("alarm");
  });

  it("reads a running project as busy", () => {
    expect(projectStatus(p({ state: "running" })).tone).toBe("busy");
  });

  /** Only two states are worth a reader's attention; the rest are quiet. */
  it("keeps ordinary states quiet", () => {
    for (const state of ["draft", "ready", "simulated"] as const) {
      expect(projectStatus(p({ state })).tone).toBe("quiet");
    }
  });

  /** Every state answers, so a row can never render an empty status. */
  it("answers for every state a project can be in", () => {
    for (const state of [
      "draft",
      "ready",
      "simulated",
      "running",
      "failed",
      "stale",
    ] as const) {
      expect(projectStatus(p({ state })).label.length).toBeGreaterThan(0);
    }
  });
});

describe("a model's size line", () => {
  it("reads as counts a person can scan", () => {
    expect(modelSize(p({ nodeCount: 1204, linkCount: 1318 }))).toBe(
      "1,204 nodes, 1,318 links",
    );
  });

  it("agrees with itself on one of something", () => {
    expect(modelSize(p({ nodeCount: 1, linkCount: 1 }))).toBe("1 node, 1 link");
  });

  /** An empty model has no size worth a line of its own. */
  it("says nothing for a model with no elements", () => {
    expect(modelSize(p({ nodeCount: 0, linkCount: 0 }))).toBeNull();
  });
});
