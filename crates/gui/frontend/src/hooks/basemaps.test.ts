/**
 * Tests for the pure offline-basemap helpers in basemaps.ts: byte
 * formatting, bbox parsing/padding, and the coverage-chip visibility
 * matrix.
 */
import { describe, expect, it, vi } from "vitest";

// basemaps.ts imports the Tauri IPC/event seams at module level; mock them
// so the pure helpers can be exercised in the node test environment.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import {
  bboxFromStrings,
  COVERAGE_MIN_ZOOM,
  formatBytes,
  isOfflineBasemap,
  padBbox,
  regionSizeLabel,
  shouldShowCoverageChip,
} from "./basemaps";

describe("formatBytes", () => {
  it("renders sub-kilobyte values in bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("steps through KB/MB/GB with one trimmed decimal", () => {
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(12.3 * 1024 * 1024)).toBe("12.3 MB");
    expect(formatBytes(1024 ** 3)).toBe("1 GB");
    expect(formatBytes(2.5 * 1024 ** 4)).toBe("2.5 TB");
  });

  it("treats invalid input as empty", () => {
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });
});

describe("regionSizeLabel", () => {
  it("combines unique and shared sizes", () => {
    expect(regionSizeLabel({ uniqueBytes: 1536, sharedBytes: 1024 ** 2 })).toBe(
      "1.5 KB unique · 1 MB shared",
    );
  });
});

describe("padBbox", () => {
  it("grows each side by the fraction of the span", () => {
    expect(padBbox([0, 0, 10, 10], 0.2)).toEqual([-2, -2, 12, 12]);
  });

  it("clamps to world bounds (lon ±180, lat ±85.05)", () => {
    const [w, s, e, n] = padBbox([-179, 84, 179, 85], 0.2);
    expect(w).toBe(-180);
    expect(e).toBe(180);
    expect(s).toBeCloseTo(83.8);
    expect(n).toBeCloseTo(85.051129);
  });
});

describe("bboxFromStrings", () => {
  it("parses four finite ordered coordinates", () => {
    expect(bboxFromStrings("-1.5", " 50 ", "1.5", "51")).toEqual([
      -1.5, 50, 1.5, 51,
    ]);
  });

  it("rejects non-numeric fields", () => {
    expect(bboxFromStrings("", "50", "1", "51")).toBeNull();
    expect(bboxFromStrings("abc", "50", "1", "51")).toBeNull();
  });

  it("rejects inverted or empty extents", () => {
    expect(bboxFromStrings("1", "50", "-1", "51")).toBeNull();
    expect(bboxFromStrings("-1", "51", "1", "50")).toBeNull();
    expect(bboxFromStrings("1", "50", "1", "51")).toBeNull();
  });

  it("rejects out-of-world coordinates", () => {
    expect(bboxFromStrings("-181", "50", "1", "51")).toBeNull();
    expect(bboxFromStrings("-1", "50", "1", "91")).toBeNull();
  });
});

describe("isOfflineBasemap", () => {
  it("matches only the offline-* styles", () => {
    expect(isOfflineBasemap("offline-streets")).toBe(true);
    expect(isOfflineBasemap("offline-dark")).toBe(true);
    expect(isOfflineBasemap("streets")).toBe(false);
    expect(isOfflineBasemap("none")).toBe(false);
  });
});

describe("shouldShowCoverageChip", () => {
  const visible = {
    basemap: "offline-streets",
    viewMode: "map",
    zoom: 12,
    covered: false as boolean | null,
    downloadActive: false,
  };

  it("shows for an uncovered viewport on an offline basemap", () => {
    expect(shouldShowCoverageChip(visible)).toBe(true);
    expect(
      shouldShowCoverageChip({ ...visible, zoom: COVERAGE_MIN_ZOOM }),
    ).toBe(true);
  });

  it("hides on online basemaps and in schematic mode", () => {
    expect(shouldShowCoverageChip({ ...visible, basemap: "streets" })).toBe(
      false,
    );
    expect(shouldShowCoverageChip({ ...visible, basemap: "none" })).toBe(false);
    expect(shouldShowCoverageChip({ ...visible, viewMode: "schematic" })).toBe(
      false,
    );
  });

  it("hides below the street-detail zoom window", () => {
    expect(
      shouldShowCoverageChip({ ...visible, zoom: COVERAGE_MIN_ZOOM - 0.1 }),
    ).toBe(false);
    expect(shouldShowCoverageChip({ ...visible, zoom: null })).toBe(false);
  });

  it("hides while coverage is unknown or complete", () => {
    expect(shouldShowCoverageChip({ ...visible, covered: null })).toBe(false);
    expect(shouldShowCoverageChip({ ...visible, covered: true })).toBe(false);
  });

  it("hides while a download is running", () => {
    expect(shouldShowCoverageChip({ ...visible, downloadActive: true })).toBe(
      false,
    );
  });
});
