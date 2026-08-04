/**
 * @vitest-environment jsdom
 *
 * Render-level regressions for id truncation. The split logic has its own
 * unit test; these cover what only the DOM can answer — how many boxes the
 * id is spread across, and what the user actually reads.
 */
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MiddleTruncate } from "./MiddleTruncate";

const boxes = (text: string) =>
  render(<MiddleTruncate text={text} />).container.querySelectorAll("span")
    .length;

describe("MiddleTruncate rendering", () => {
  // The bug: `Street1` is one character longer than the pinned tail, so it
  // was split into a one-character head — and the head carries a 2ch floor
  // to keep the ellipsis paintable. The floor padded "S" out to two
  // characters' width, and the gap read as part of the id: "S treet1".
  it("renders a short id in a single box, so no floor can pad it", () => {
    expect(boxes("Street1")).toBe(1);
    expect(boxes("Streets1")).toBe(1);
  });

  it("splits a long id into head and tail", () => {
    expect(boxes("WMTR-G1209")).toBeGreaterThan(1);
  });

  it("shows the id exactly, split or not", () => {
    for (const id of ["Street1", "WMTR-G1209", "J1"]) {
      const { container } = render(<MiddleTruncate text={id} />);
      expect(container.textContent).toBe(id);
    }
  });

  it("carries the full id as a tooltip either way", () => {
    for (const id of ["Street1", "WMTR-G1209"]) {
      const { container } = render(<MiddleTruncate text={id} />);
      expect(container.querySelector("span")?.title).toBe(id);
    }
  });
});
