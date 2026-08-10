import { describe, expect, it } from "vitest";
import {
  downloadPercent,
  mockUpdateVersion,
  shouldCheckOnLaunch,
} from "./useUpdater";

describe("downloadPercent", () => {
  it("computes whole-number percentages", () => {
    expect(downloadPercent(0, 200)).toBe(0);
    expect(downloadPercent(50, 200)).toBe(25);
    expect(downloadPercent(199, 200)).toBe(100); // rounds
    expect(downloadPercent(200, 200)).toBe(100);
  });

  it("clamps overshoot to 100", () => {
    // Servers can send more bytes than the advertised content length
    // (e.g. stale metadata) — the bar must never exceed 100.
    expect(downloadPercent(250, 200)).toBe(100);
  });

  it("is indeterminate (null) without a usable total", () => {
    expect(downloadPercent(50, null)).toBeNull();
    expect(downloadPercent(50, 0)).toBeNull();
    expect(downloadPercent(50, -5)).toBeNull();
    expect(downloadPercent(50, Number.NaN)).toBeNull();
    expect(downloadPercent(50, Number.POSITIVE_INFINITY)).toBeNull();
  });
});

describe("mockUpdateVersion", () => {
  it("accepts plain dotted versions", () => {
    expect(mockUpdateVersion("9.9.9")).toBe("9.9.9");
    expect(mockUpdateVersion("3.0")).toBe("3.0");
    expect(mockUpdateVersion("10")).toBe("10");
    expect(mockUpdateVersion("  2.5.0  ")).toBe("2.5.0");
  });

  it("rejects absent or malformed markers", () => {
    expect(mockUpdateVersion(null)).toBeNull();
    expect(mockUpdateVersion("")).toBeNull();
    expect(mockUpdateVersion("v2.5.0")).toBeNull();
    expect(mockUpdateVersion("2.5.0-beta")).toBeNull();
    expect(mockUpdateVersion("not a version")).toBeNull();
    expect(mockUpdateVersion("1.2.3.4")).toBeNull();
  });
});

describe("shouldCheckOnLaunch", () => {
  it("checks on a fresh install, where nothing has been stored", () => {
    expect(shouldCheckOnLaunch(null)).toBe(true);
  });

  it("does not check once the reader has said so", () => {
    // The check is a network call to GitHub made before the user has done
    // anything, so declining it has to actually stop it.
    expect(shouldCheckOnLaunch("false")).toBe(false);
  });

  it("checks when the stored value is anything else", () => {
    // A corrupted preference is not a reason to strand someone on an old
    // build: only an explicit refusal counts as one.
    expect(shouldCheckOnLaunch("true")).toBe(true);
    expect(shouldCheckOnLaunch("")).toBe(true);
    expect(shouldCheckOnLaunch("no")).toBe(true);
    expect(shouldCheckOnLaunch("0")).toBe(true);
  });
});
