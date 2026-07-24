import { describe, expect, it } from "vitest";
import { textToTitleLines, titleLinesToText } from "./modelTitle";

describe("titleLinesToText / textToTitleLines", () => {
  it("round-trips multi-line titles", () => {
    const lines = ["Main title", "detail one", "", "detail three"];
    expect(textToTitleLines(titleLinesToText(lines))).toEqual(lines);
  });

  it("drops trailing empties and per-line trailing whitespace", () => {
    expect(textToTitleLines("A  \nB\n\n\n")).toEqual(["A", "B"]);
  });

  it("keeps interior empty lines", () => {
    expect(textToTitleLines("A\n\nC")).toEqual(["A", "", "C"]);
  });

  it("empty text yields no lines", () => {
    expect(textToTitleLines("")).toEqual([]);
    expect(textToTitleLines("   \n ")).toEqual([]);
  });

  it("does not cap line count (EPANET 3 lines is convention only)", () => {
    expect(textToTitleLines("1\n2\n3\n4\n5")).toHaveLength(5);
  });
});
