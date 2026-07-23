/**
 * Tests for the hooks barrel (hooks/index.ts) — exercises functions that
 * have logic independent of the Tauri runtime (fallback paths, default
 * values, etc.).
 *
 * The `@tauri-apps/api/core` module is mocked so `tryInvoke` either returns
 * `null` (simulating the non-Tauri browser path) or a controlled value.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Mock @tauri-apps/api/core before any data/ imports ────────────────────────
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// Import after mock is established.
import { invoke } from "@tauri-apps/api/core";
import {
  cancelRunQueue,
  enqueueRuns,
  fetchProjectsShared,
  getVersions,
  reconcileProjects,
  type SimParams,
  updateSimParams,
} from "./index";
import { formatIpcError, onIpcError, tryInvoke, tryInvokeOr } from "./ipc";

const mockInvoke = vi.mocked(invoke);

/** Make `isTauri()` return true for the current test. */
function stubTauriShell() {
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
}

beforeEach(() => {
  // Reset call history but don't change implementation between tests.
  mockInvoke.mockReset();
  // Default: behave as if outside Tauri (window.__TAURI_INTERNALS__ absent).
  // isTauri() reads `window`, which is `undefined` in the Node environment,
  // so tryInvoke will return null by default without any special setup.
});

afterEach(() => {
  vi.unstubAllGlobals();
});

// ── reconcileProjects ─────────────────────────────────────────────────────────

describe("reconcileProjects", () => {
  it("returns the fallback value when not inside a Tauri shell", async () => {
    // Running in Node → isTauri() returns false → tryInvoke returns null
    // → reconcileProjects falls back to { recovered: 0, folderMissing: [] }
    const result = await reconcileProjects();
    expect(result.recovered).toBe(0);
    expect(result.folderMissing).toEqual([]);
  });

  it("fallback folderMissing is an array (not null/undefined)", async () => {
    const result = await reconcileProjects();
    expect(Array.isArray(result.folderMissing)).toBe(true);
  });
});

// ── getVersions ───────────────────────────────────────────────────────────────

describe("getVersions", () => {
  it("returns fallback versions when not inside a Tauri shell", async () => {
    const result = await getVersions();
    expect(result.hydra).toBe("0.0.0");
    expect(result.app).toBe("0.0.0");
  });
});

// ── tryInvoke / onIpcError ────────────────────────────────────────────────────

describe("tryInvoke error reporting", () => {
  it("outside Tauri: resolves null without invoking or reporting", async () => {
    const handler = vi.fn();
    const off = onIpcError(handler);
    const result = await tryInvoke("list_projects");
    off();
    expect(result).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(handler).not.toHaveBeenCalled();
  });

  it("inside Tauri: a rejected command resolves null AND reports to onIpcError", async () => {
    stubTauriShell();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockInvoke.mockRejectedValueOnce("db is corrupted");
    const handler = vi.fn();
    const off = onIpcError(handler);
    const result = await tryInvoke("list_projects");
    off();
    warn.mockRestore();
    expect(result).toBeNull();
    expect(handler).toHaveBeenCalledWith("list_projects", "db is corrupted");
  });

  it("unregistering the handler stops reporting", async () => {
    stubTauriShell();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockInvoke.mockRejectedValueOnce("boom");
    const handler = vi.fn();
    onIpcError(handler)();
    await tryInvoke("list_projects");
    warn.mockRestore();
    expect(handler).not.toHaveBeenCalled();
  });
});

describe("tryInvokeOr", () => {
  it("outside Tauri: resolves the fallback without invoking", async () => {
    const result = await tryInvokeOr<string[]>("list_scenarios", undefined, []);
    expect(result).toEqual([]);
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("inside Tauri: resolves the command's value when it succeeds", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce(["s1"]);
    const result = await tryInvokeOr<string[]>(
      "list_scenarios",
      { projectId: "p1" },
      [],
    );
    expect(result).toEqual(["s1"]);
    expect(mockInvoke).toHaveBeenCalledWith("list_scenarios", {
      projectId: "p1",
    });
  });

  it("inside Tauri: a rejected command resolves the fallback", async () => {
    stubTauriShell();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    mockInvoke.mockRejectedValueOnce("boom");
    const result = await tryInvokeOr<boolean>("delete_scenario", {}, false);
    warn.mockRestore();
    expect(result).toBe(false);
  });

  it("a null command result maps to the fallback", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce(null);
    const result = await tryInvokeOr<number>("get_count", undefined, 7);
    expect(result).toBe(7);
  });
});

describe("formatIpcError", () => {
  it("unwraps Error messages and passes strings through", () => {
    expect(formatIpcError(new Error("nope"))).toBe("nope");
    expect(formatIpcError("plain string")).toBe("plain string");
    expect(formatIpcError({ code: 1 })).toBe('{"code":1}');
  });
});

// ── Mutating queue / sim-params commands propagate errors ─────────────────────

describe("mutating commands propagate backend errors", () => {
  it("enqueueRuns rejects when the backend command fails", async () => {
    stubTauriShell();
    mockInvoke.mockRejectedValueOnce("queue is locked");
    await expect(enqueueRuns("p1", [null])).rejects.toBe("queue is locked");
  });

  it("cancelRunQueue rejects when the backend command fails", async () => {
    stubTauriShell();
    mockInvoke.mockRejectedValueOnce("no such project");
    await expect(cancelRunQueue("p1")).rejects.toBe("no such project");
  });

  it("updateSimParams rejects when the backend command fails", async () => {
    stubTauriShell();
    mockInvoke.mockRejectedValueOnce("read-only filesystem");
    await expect(updateSimParams("p1", {} as SimParams)).rejects.toBe(
      "read-only filesystem",
    );
  });

  it("enqueueRuns rejects outside a Tauri shell (no silent no-op)", async () => {
    await expect(enqueueRuns("p1", [null])).rejects.toThrow(
      "Not running inside Tauri",
    );
  });
});

// ── list_projects dedup ───────────────────────────────────────────────────────

describe("fetchProjectsShared", () => {
  it("shares one in-flight list_projects invoke between concurrent callers", async () => {
    stubTauriShell();
    let resolve: (rows: unknown) => void = () => {};
    mockInvoke.mockImplementationOnce(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const a = fetchProjectsShared();
    const b = fetchProjectsShared();
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    resolve([{ id: "p1" }]);
    const [ra, rb] = await Promise.all([a, b]);
    expect(ra).toEqual([{ id: "p1" }]);
    expect(rb).toBe(ra);
  });

  it("issues a fresh invoke once the previous fetch settled", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce([]);
    await fetchProjectsShared();
    mockInvoke.mockResolvedValueOnce([]);
    await fetchProjectsShared();
    expect(mockInvoke).toHaveBeenCalledTimes(2);
  });
});
