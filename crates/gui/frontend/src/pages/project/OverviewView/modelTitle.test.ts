import { describe, expect, it } from "vitest";
import { partsToTitleLines, titleLinesToParts } from "./modelTitle";

describe("titleLinesToParts", () => {
  it("splits first line from description lines", () => {
    expect(titleLinesToParts(["Main", "d1", "d2"])).toEqual({
      title: "Main",
      description: "d1\nd2",
    });
  });

  it("handles empty and single-line titles", () => {
    expect(titleLinesToParts([])).toEqual({ title: "", description: "" });
    expect(titleLinesToParts(["Only"])).toEqual({
      title: "Only",
      description: "",
    });
  });
});

describe("partsToTitleLines", () => {
  it("joins title and description into at most three lines", () => {
    expect(partsToTitleLines({ title: "Main", description: "d1\nd2" })).toEqual(
      ["Main", "d1", "d2"],
    );
  });

  it("collapses overflow description lines into line three", () => {
    expect(
      partsToTitleLines({ title: "T", description: "a\nb\nc\nd" }),
    ).toEqual(["T", "a", "b c d"]);
  });

  it("drops trailing empties; empty card yields empty title", () => {
    expect(partsToTitleLines({ title: "T", description: "" })).toEqual(["T"]);
    expect(partsToTitleLines({ title: "", description: "" })).toEqual([]);
    expect(partsToTitleLines({ title: "", description: "only desc" })).toEqual([
      "",
      "only desc",
    ]);
  });

  it("round-trips with titleLinesToParts", () => {
    const lines = ["Main title", "line two", "line three"];
    expect(partsToTitleLines(titleLinesToParts(lines))).toEqual(lines);
  });
});
