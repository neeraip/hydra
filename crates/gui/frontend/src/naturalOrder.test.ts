/**
 * Ordering text the way a reader reads it.
 *
 * Element ids in these models are usually numbers written as text, and
 * comparing characters lists them 1, 10, 11, 2 — the order of the
 * characters and of nothing anyone is looking for. It showed in every
 * sorted column of the Editor's tables and in every datalist a reference
 * field drops down.
 */
import { describe, expect, it } from "vitest";
import { compareNatural } from "./naturalOrder";

describe("compareNatural", () => {
  it("reads digits as numbers", () => {
    expect(["10", "9", "2", "100"].sort(compareNatural)).toEqual([
      "2",
      "9",
      "10",
      "100",
    ]);
  });

  it("reads digits inside a name too", () => {
    expect(["C10", "C2", "C1"].sort(compareNatural)).toEqual([
      "C1",
      "C2",
      "C10",
    ]);
  });

  it("does not split a list on case alone", () => {
    // The engines resolve ids case-insensitively, so a reader looking for
    // `p1` should not have to know whether the file capitalised it — and
    // an uppercase block sitting above a lowercase one is what a
    // character comparison gives.
    expect(["p2", "P1", "p3"].sort(compareNatural)).toEqual(["P1", "p2", "p3"]);
  });

  it("still orders text that is only text", () => {
    expect(["Tank", "Junction", "Pipe"].sort(compareNatural)).toEqual([
      "Junction",
      "Pipe",
      "Tank",
    ]);
  });
});
