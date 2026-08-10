import { describe, expect, it } from "vitest";
import {
  type DataUsage,
  describeCleared,
  describeUsage,
  formatBytes,
} from "./storage";

describe("formatBytes", () => {
  it("reads the way a file manager does", () => {
    expect(formatBytes(0)).toBe("0 bytes");
    expect(formatBytes(512)).toBe("512 bytes");
    expect(formatBytes(1024)).toBe("1.0 kB");
    expect(formatBytes(1536)).toBe("1.5 kB");
    expect(formatBytes(1024 ** 3 * 1.4)).toBe("1.4 GB");
  });

  it("stops at a unit it can name", () => {
    // Beyond terabytes the loop must not walk off the end of the list and
    // print "undefined".
    expect(formatBytes(1024 ** 6)).toBe("1048576.0 TB");
  });

  it("treats nonsense as nothing rather than as NaN", () => {
    expect(formatBytes(Number.NaN)).toBe("0 bytes");
    expect(formatBytes(-5)).toBe("0 bytes");
  });
});

function usage(over: Partial<DataUsage> = {}): DataUsage {
  return {
    totalBytes: 1024 ** 3 * 4,
    resultsBytes: 1024 ** 3 * 3,
    projectCount: 7,
    ...over,
  };
}

describe("describeUsage", () => {
  it("says the results are part of the total, not beside it", () => {
    // Two bare figures invite the wrong subtraction: a reader seeing 4 GB
    // and 3 GB cannot tell whether one contains the other.
    expect(describeUsage(usage())).toBe(
      "4.0 GB across 7 projects, of which 3.0 GB is simulation results.",
    );
  });

  it("counts one project singly", () => {
    expect(describeUsage(usage({ projectCount: 1 }))).toContain("1 project,");
  });

  it("says so plainly on a fresh install", () => {
    expect(describeUsage(usage({ projectCount: 0 }))).toBe(
      "Nothing stored yet.",
    );
  });

  it("does not claim a figure it has not got", () => {
    expect(describeUsage(null)).toBe("Measuring…");
  });
});

describe("describeCleared", () => {
  it("reports what went", () => {
    expect(describeCleared({ removed: 4, skipped: 0 })).toBe(
      "4 results cleared.",
    );
    expect(describeCleared({ removed: 1, skipped: 0 })).toBe(
      "1 result cleared.",
    );
  });

  it("says which projects were left, and why", () => {
    // A silent partial clear is the worst outcome: the folder is smaller,
    // the reader believes it is empty, and nothing says otherwise.
    expect(describeCleared({ removed: 2, skipped: 1 })).toBe(
      "2 results cleared. 1 project was left alone — a simulation is running.",
    );
    expect(describeCleared({ removed: 0, skipped: 3 })).toBe(
      "0 results cleared. 3 projects were left alone — a simulation is running.",
    );
  });
});
