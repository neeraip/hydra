import { describe, expect, it } from "vitest";
import type { Task } from "../types/task";
import {
  backfillTask,
  type QueueItemFacts,
  taskNeedsBackfill,
} from "./taskBackfill";

const facts: QueueItemFacts = {
  projectId: "proj-1",
  targetId: "scen-9",
  projectName: "North Zone",
  targetName: "High demand",
};

/** The row the progress-event timing race produces: no names, no identity. */
const placeholder: Task = {
  id: "queue-run-1",
  projectName: "…",
  scenarioName: "…",
  status: "running",
  timeLabel: "Running…",
};

describe("taskBackfill", () => {
  it("fills identity and names on a fresh placeholder", () => {
    const filled = backfillTask(placeholder, facts);
    expect(filled.projectId).toBe("proj-1");
    expect(filled.scenarioId).toBe("scen-9");
    expect(filled.projectName).toBe("North Zone");
    expect(filled.scenarioName).toBe("High demand");
  });

  it("still backfills identity after the names were already patched", () => {
    // The regression: a task patched by an earlier pass has real names but
    // no projectId, and a name-only predicate skipped it forever — leaving
    // "View results" permanently dead.
    const named: Task = {
      ...placeholder,
      projectName: "North Zone",
      scenarioName: "High demand",
    };
    expect(taskNeedsBackfill(named)).toBe(true);
    expect(backfillTask(named, facts).projectId).toBe("proj-1");
  });

  it("treats a fully populated task as needing nothing", () => {
    const complete: Task = {
      ...placeholder,
      projectId: "proj-1",
      scenarioId: null,
      projectName: "North Zone",
      scenarioName: "Base Model",
    };
    expect(taskNeedsBackfill(complete)).toBe(false);
    expect(backfillTask(complete, facts)).toBe(complete);
  });

  it("keeps a null scenarioId, which means the base model", () => {
    // `null` is a real target, not an absent one. Coalescing it would
    // silently redirect a base-model run's "View results" at a scenario.
    const baseRun: Task = {
      ...placeholder,
      projectName: "North Zone",
      scenarioId: null,
    };
    expect(backfillTask(baseRun, facts).scenarioId).toBeNull();
  });

  it("never overwrites values the task already has", () => {
    const live: Task = {
      ...placeholder,
      projectId: "proj-other",
      scenarioId: "scen-other",
      projectName: "Its Own Name",
      scenarioName: "…",
    };
    const filled = backfillTask(live, facts);
    expect(filled.projectId).toBe("proj-other");
    expect(filled.scenarioId).toBe("scen-other");
    expect(filled.projectName).toBe("Its Own Name");
    // Only the genuinely absent field is filled.
    expect(filled.scenarioName).toBe("High demand");
  });

  it("falls back to Base Model when the queue item has no target name", () => {
    const filled = backfillTask(placeholder, { ...facts, targetName: null });
    expect(filled.scenarioName).toBe("Base Model");
  });
});
