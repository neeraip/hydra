import { afterEach, describe, expect, it, vi } from "vitest";

// Mock the Tauri IPC seam so we can drive success/rejection without a shell.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  formatIpcError,
  invoke,
  onIpcError,
  tryInvoke,
  tryInvokeOr,
} from "./ipc";

const mockInvoke = vi.mocked(tauriInvoke);

/** Make `isTauri()` return true for the current test. */
function stubTauriShell() {
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
}

afterEach(() => {
  vi.unstubAllGlobals();
  mockInvoke.mockReset();
});

describe("formatIpcError", () => {
  it("renders Error, string, and object errors", () => {
    expect(formatIpcError(new Error("boom"))).toBe("boom");
    expect(formatIpcError("nope")).toBe("nope");
    expect(formatIpcError({ code: 5 })).toBe('{"code":5}');
  });
});

describe("outside a Tauri shell", () => {
  it("tryInvoke resolves null without calling the backend", async () => {
    await expect(tryInvoke("cmd")).resolves.toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("tryInvokeOr resolves the provided fallback", async () => {
    await expect(tryInvokeOr("cmd", undefined, 42)).resolves.toBe(42);
  });

  it("invoke rejects (the strict variant)", async () => {
    await expect(invoke("cmd")).rejects.toThrow(/Not running inside Tauri/);
  });
});

describe("inside a Tauri shell", () => {
  it("tryInvoke returns the backend value on success", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce("ok");
    await expect(tryInvoke("cmd")).resolves.toBe("ok");
    expect(mockInvoke).toHaveBeenCalledWith("cmd", undefined);
  });

  it("tryInvoke reports a rejection to onIpcError and resolves null", async () => {
    stubTauriShell();
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const handler = vi.fn();
    const unregister = onIpcError(handler);
    mockInvoke.mockRejectedValueOnce("backend exploded");

    await expect(tryInvoke("do_thing")).resolves.toBeNull();
    expect(handler).toHaveBeenCalledWith("do_thing", "backend exploded");
    unregister();
  });

  it("tryInvokeOr falls back when the backend returns null", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce(null);
    await expect(tryInvokeOr("cmd", undefined, "fallback")).resolves.toBe(
      "fallback",
    );
  });
});
