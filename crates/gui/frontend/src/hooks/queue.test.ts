import { describe, expect, it } from "vitest";

import { type RunQueueItem, resumableTargets } from "./queue";

function item(over: Partial<RunQueueItem>): RunQueueItem {
  return {
    id: "r1",
    projectId: "p1",
    targetId: null,
    targetName: null,
    status: "cancelled",
    resume: false,
    resumable: false,
    queuedAt: 1,
    startedAt: null,
    finishedAt: null,
    error: null,
    ...over,
  };
}

describe("resumableTargets", () => {
  it("offers the base model when its run was interrupted", () => {
    const targets = resumableTargets([item({ resumable: true })]);
    expect(targets.has(null)).toBe(true);
    expect(targets.size).toBe(1);
  });

  it("keeps scenarios apart from the base model", () => {
    const targets = resumableTargets([
      item({ id: "a", targetId: null, resumable: true }),
      item({ id: "b", targetId: "s1", resumable: false }),
      item({ id: "c", targetId: "s2", resumable: true }),
    ]);
    expect([...targets].sort()).toEqual([null, "s2"]);
  });

  it("lets the newest item for a target decide", () => {
    // Cancelled, then run again to completion: the finished run cleared
    // the checkpoint, so the older item's offer is stale. Reading every
    // item instead would keep offering to resume a run that has since
    // finished, and taking it would discard the finished results.
    const targets = resumableTargets([
      item({ id: "old", targetId: "s1", resumable: true, queuedAt: 1 }),
      item({
        id: "new",
        targetId: "s1",
        status: "done",
        resumable: false,
        queuedAt: 2,
      }),
    ]);
    expect(targets.size).toBe(0);
  });

  it("offers again when the newest item is itself interrupted", () => {
    const targets = resumableTargets([
      item({ id: "old", targetId: "s1", status: "done", queuedAt: 1 }),
      item({ id: "new", targetId: "s1", resumable: true, queuedAt: 2 }),
    ]);
    expect([...targets]).toEqual(["s1"]);
  });

  it("offers nothing for an empty queue", () => {
    expect(resumableTargets([]).size).toBe(0);
  });
});
