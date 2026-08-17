/**
 * The seed for an added contents row (hydra-common §4.5.2.2).
 *
 * The defect this guards: the add button appended a row of zeros, and a
 * table whose advancing column had passed zero — every curve, every
 * series — could only refuse it, so the button always failed. The seed
 * has to land, which means past the last row in the column the engine
 * says must advance.
 */
import { describe, expect, it } from "vitest";
import { nextRow } from "./nextRow";

describe("nextRow", () => {
  it("moves the advancing column past the last row, by the table's own step", () => {
    // An hourly series stays hourly; a half-hourly one half-hourly.
    expect(
      nextRow(
        [
          [0, 5],
          [0.5, 7],
        ],
        2,
        0,
      ),
    ).toEqual([1, 7]);
  });

  it("advances by one when a single row gives no step to copy", () => {
    expect(nextRow([[3, 9]], 2, 0)).toEqual([4, 9]);
  });

  it("advances whichever column the engine named", () => {
    // A transect's station is its second value, not its first.
    expect(
      nextRow(
        [
          [2, 0],
          [0, 5],
        ],
        2,
        1,
      ),
    ).toEqual([0, 10]);
  });

  it("copies the last row when nothing has to advance", () => {
    // A pattern: the interval column is the read's own numbering, so the
    // copied row is legal and the refetch renumbers it.
    expect(nextRow([[1, 1.2]], 2, undefined)).toEqual([1, 1.2]);
  });

  it("falls back to a positive step when the last step was not one", () => {
    // A table that was edited out of order still gets a row that lands.
    expect(
      nextRow(
        [
          [5, 1],
          [5.5, 2],
        ],
        2,
        0,
      ),
    ).toEqual([6, 2]);
    expect(
      nextRow(
        [
          [8, 1],
          [6, 2],
        ],
        2,
        0,
      ),
    ).toEqual([7, 2]);
  });

  it("seeds zeros for an empty table", () => {
    expect(nextRow([], 2, 0)).toEqual([0, 0]);
  });
});
