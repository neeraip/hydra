/**
 * What becomes of a save, and whether anyone is told.
 *
 * The write flow exists to stop an edit living only in memory, because
 * one that does is lost when the app closes and the user has no way to
 * know. It made sure the save was *called* and nothing made sure it
 * worked: the command's failure was swallowed on the way past, arrived
 * as the same `false` a draft project with no model returns, and every
 * caller dropped it.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeResult = vi.fn<() => Promise<{ ok: boolean; value?: boolean }>>();
const tauri = vi.fn(() => true);

vi.mock("./ipc", () => ({
  isTauri: () => tauri(),
  invoke: () => Promise.resolve(),
  tryInvoke: () => Promise.resolve(null),
  tryInvokeOr: (_c: string, _a: unknown, fallback: unknown) =>
    Promise.resolve(fallback),
  tryInvokeResult: () => invokeResult(),
}));

const { SAVE_FAILED_MESSAGE, persistOrSay, saveProjectOnDisk } = await import(
  "./projects"
);

beforeEach(() => {
  tauri.mockReturnValue(true);
  invokeResult.mockResolvedValue({ ok: true, value: true });
});

describe("saveProjectOnDisk", () => {
  it("says it saved when the model was written", async () => {
    expect(await saveProjectOnDisk("p1", null)).toBe("saved");
  });

  it("tells a draft with no model apart from a save that failed", async () => {
    // The two used to arrive as the same `false`. One is a project that
    // has nothing to write yet; the other is an edit that is not on
    // disk, and only the second is worth interrupting anyone about.
    invokeResult.mockResolvedValue({ ok: true, value: false });
    expect(await saveProjectOnDisk("p1", null)).toBe("nothing-to-save");

    invokeResult.mockResolvedValue({ ok: false });
    expect(await saveProjectOnDisk("p1", null)).toBe("failed");
  });

  it("is the empty case outside Tauri, not a failure", async () => {
    // No disk to write to and no edit to lose: nothing is loaded.
    tauri.mockReturnValue(false);
    expect(await saveProjectOnDisk("p1", null)).toBe("nothing-to-save");
  });
});

describe("persistOrSay", () => {
  it("says nothing when the save worked", async () => {
    const toast = vi.fn();
    await persistOrSay("p1", null, toast);
    expect(toast).not.toHaveBeenCalled();
  });

  it("says nothing when there was nothing to save", async () => {
    // A draft project is not a failure, and a toast on every edit of one
    // would teach people to ignore the toast that matters.
    invokeResult.mockResolvedValue({ ok: true, value: false });
    const toast = vi.fn();
    await persistOrSay("p1", null, toast);
    expect(toast).not.toHaveBeenCalled();
  });

  it("says so when the edit is in the model and not on disk", async () => {
    invokeResult.mockResolvedValue({ ok: false });
    const toast = vi.fn();
    await persistOrSay("p1", null, toast);
    expect(toast).toHaveBeenCalledWith(SAVE_FAILED_MESSAGE, "error");
    // The sentence names both halves, because the difference is the
    // point: the edit is real, and closing the app now loses it.
    expect(SAVE_FAILED_MESSAGE).toMatch(/in the model/);
    expect(SAVE_FAILED_MESSAGE).toMatch(/lost if the app closes/);
  });
});
