import { describe, expect, it } from "vitest";
import {
  ELEMENT_FINDER_PREFIX,
  elementFinderSeed,
  elementFinderTerm,
  isElementFinderQuery,
} from "./elementFinder";

/**
 * Reaching the element search meant opening the palette and picking "Find
 * an element on canvas…" from the command list first. It is now also a
 * keystroke, which gives the marker that opens the mode a third reader
 * after the palette's own mode check and that helper command.
 *
 * Three literal `"#"`s is the shape that has gone wrong repeatedly here:
 * copies that agree until one is changed. So the marker is named once and
 * the routes in are asserted to land in the same place.
 */

describe("the finder's marker", () => {
  it("seeds the mode and nothing more", () => {
    expect(elementFinderSeed()).toBe(ELEMENT_FINDER_PREFIX);
  });

  /**
   * The load-bearing one. The shortcut and the menu command both seed the
   * palette, and the palette decides what to show by reading the query
   * back — so a seed the mode check does not recognise opens the palette on
   * a command list with a stray character in it.
   */
  it("opens a query the palette reads as element search", () => {
    expect(isElementFinderQuery(elementFinderSeed())).toBe(true);
  });

  it("asks for nothing in particular until something is typed", () => {
    expect(elementFinderTerm(elementFinderSeed())).toBe("");
  });
});

describe("reading a query", () => {
  it("recognises the mode", () => {
    expect(isElementFinderQuery("#J31")).toBe(true);
    expect(isElementFinderQuery("run")).toBe(false);
    expect(isElementFinderQuery("")).toBe(false);
  });

  /** A marker part-way through is someone typing, not a mode. */
  it("only recognises it at the start", () => {
    expect(isElementFinderQuery("pipe #4")).toBe(false);
  });

  it("strips the marker and the space around it", () => {
    expect(elementFinderTerm("#  J31  ")).toBe("j31");
  });

  /** Ids match case-insensitively, so doing it here saves every caller
   *  remembering to. */
  it("lowercases the term", () => {
    expect(elementFinderTerm("#P374")).toBe("p374");
  });

  it("has nothing to search for outside the mode", () => {
    expect(elementFinderTerm("run simulation")).toBe("");
  });
});
