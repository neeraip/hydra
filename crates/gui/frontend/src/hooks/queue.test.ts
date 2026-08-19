import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./ipc", () => ({
  invoke: vi.fn(),
  tryInvokeOr: vi.fn(),
}));

import { tryInvokeOr } from "./ipc";
import { resumableTargets } from "./queue";

describe("resumableTargets", () => {
  beforeEach(() => {
    vi.mocked(tryInvokeOr).mockReset();
  });

  it("asks the backend, which reads the checkpoints on disk", async () => {
    // Not derived from the run queue: a run interrupted by closing the
    // application leaves a checkpoint and no queue item, and that is the
    // case this feature exists for.
    vi.mocked(tryInvokeOr).mockResolvedValue([null, "s1"]);
    const targets = await resumableTargets("p1");
    expect(targets).toEqual([null, "s1"]);
    expect(tryInvokeOr).toHaveBeenCalledWith(
      "resumable_targets",
      { projectId: "p1" },
      [],
    );
  });

  it("offers nothing outside a Tauri shell", async () => {
    // tryInvokeOr yields its fallback when there is no backend, so a
    // browser preview shows no resume offers rather than failing.
    vi.mocked(tryInvokeOr).mockImplementation(
      async (_cmd: string, _args: unknown, fallback: unknown) => fallback,
    );
    expect(await resumableTargets("p1")).toEqual([]);
  });
});
