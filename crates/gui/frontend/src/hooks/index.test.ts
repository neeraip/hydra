/**
 * Tests for data/index.ts — exercises functions that have logic independent
 * of the Tauri runtime (fallback paths, default values, etc.).
 *
 * The `@tauri-apps/api/core` module is mocked so `tryInvoke` either returns
 * `null` (simulating the non-Tauri browser path) or a controlled value.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

// ── Mock @tauri-apps/api/core before any data/ imports ────────────────────────
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// Import after mock is established.
import { invoke } from "@tauri-apps/api/core";
import { getVersions, reconcileProjects } from "./index";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  // Reset call history but don't change implementation between tests.
  mockInvoke.mockReset();
  // Default: behave as if outside Tauri (window.__TAURI_INTERNALS__ absent).
  // isTauri() reads `window`, which is `undefined` in the Node environment,
  // so tryInvoke will return null by default without any special setup.
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
