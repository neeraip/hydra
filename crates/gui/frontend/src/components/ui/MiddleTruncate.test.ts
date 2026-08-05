import { describe, expect, it } from "vitest";
import { splitForTruncation } from "./MiddleTruncate";

const TAIL = 6;

describe("splitForTruncation", () => {
  // The head is floored at a width that keeps the ellipsis paintable, so a
  // head narrower than that floor is padded out to it — a gap that reads as
  // part of the id. `Street1` rendered as "S treet1" in the network list.
  it("declines to split when the head would be narrower than the floor", () => {
    expect(splitForTruncation("Street1", TAIL)).toBeNull(); // head "S"
    expect(splitForTruncation("Streets1", TAIL)).toBeNull(); // head "St"
  });

  it("declines to split an id shorter than the tail", () => {
    expect(splitForTruncation("J1", TAIL)).toBeNull();
    expect(splitForTruncation("", TAIL)).toBeNull();
  });

  it("splits once the head is wide enough to elide", () => {
    expect(splitForTruncation("Streetsq1", TAIL)).toEqual({
      head: "Str",
      tail: "eetsq1",
    });
  });

  // The whole point of eliding the head: shared prefixes carry no
  // information, the suffix is what tells two rows apart.
  it("pins the discriminating tail and elides the shared prefix", () => {
    expect(splitForTruncation("WMTR-G1209", TAIL)).toEqual({
      head: "WMTR",
      tail: "G1209".padStart(6, "-"),
    });
  });

  it("loses no characters when it splits", () => {
    for (const id of ["WMTR-G1209", "Streetsq1", "abcdefghijklmnop"]) {
      const s = splitForTruncation(id, TAIL);
      expect(s && s.head + s.tail).toBe(id);
    }
  });

  it("honours a custom tail length", () => {
    expect(splitForTruncation("ABCDEF", 2)).toEqual({
      head: "ABCD",
      tail: "EF",
    });
    // Same id, longer tail: no head left to elide.
    expect(splitForTruncation("ABCDEF", 5)).toBeNull();
  });
});
