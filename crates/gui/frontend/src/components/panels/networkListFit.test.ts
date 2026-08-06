import { describe, expect, it } from "vitest";
import type { Row } from "./NetworkList";
import { fitContent } from "./networkListFit";

/**
 * Picking the content that sets the panel's width.
 *
 * The property worth guarding is not just "returns the longest id" — it
 * is that the answer never depends on formatting or measuring anything
 * per row, because that is what would make fitting a 50k-element list
 * expensive rather than free.
 */

const row = (over: Partial<Row>): Row => ({
  id: "J1",
  kind: "junction",
  cls: "point",
  context: "",
  value: null,
  format: null,
  canZoom: false,
  ...over,
});

describe("the content a fit would have to show", () => {
  it("finds the longest id", () => {
    const rows = [row({ id: "J1" }), row({ id: "J-100-A" }), row({ id: "J2" })];
    expect(fitContent(rows, false)?.id).toBe("J-100-A");
  });

  /**
   * The subtitle is only drawn while searching, and reserving room for a
   * line nobody can see is just a narrower list.
   */
  it("ignores the subtitle when it is not being shown", () => {
    const rows = [row({ id: "J1", context: "a very long context indeed" })];
    expect(fitContent(rows, false)?.context).toBeNull();
    expect(fitContent(rows, true)?.context).toBe("a very long context indeed");
  });

  /**
   * The two lines are set at different sizes, so the longer string is not
   * necessarily the wider one. Both travel, and the renderer decides.
   */
  it("reports both lines rather than reducing them to one", () => {
    const fit = fitContent(
      [row({ id: "short", context: "much longer than the id" })],
      true,
    );
    expect(fit?.id).toBe("short");
    expect(fit?.context).toBe("much longer than the id");
  });

  /**
   * The value lane is bounded by its extremes, so a caller formats two
   * numbers instead of every row. A negative minimum matters: the minus
   * sign is a character the maximum does not have.
   */
  it("reports the extreme values, not every value", () => {
    const rows = [
      row({ value: 5 }),
      row({ value: -120.5 }),
      row({ value: 42 }),
    ];
    expect(fitContent(rows, false)?.extremes).toEqual([-120.5, 42]);
  });

  it("reports no extremes before a run has produced values", () => {
    expect(fitContent([row({}), row({})], false)?.extremes).toBeNull();
  });

  /** Rows without a value must not drag the extremes toward zero. */
  it("ignores rows carrying no value", () => {
    const rows = [row({ value: null }), row({ value: 7 }), row({ value: 9 })];
    expect(fitContent(rows, false)?.extremes).toEqual([7, 9]);
  });

  /** The zoom affordance widens a row's padding, so a fit that ignores it
   *  is narrow by exactly that padding on the rows that have it. */
  it("notices whether any row carries the zoom affordance", () => {
    expect(fitContent([row({}), row({})], false)?.zoomable).toBe(false);
    expect(fitContent([row({}), row({ canZoom: true })], false)?.zoomable).toBe(
      true,
    );
  });

  it("has nothing to fit for an empty list", () => {
    expect(fitContent([], false)).toBeNull();
  });

  /**
   * The cost claim, asserted rather than assumed: the pass must not build
   * a string per row. A row whose `id` getter counts reads shows how many
   * times each is touched — once — and nothing formats the values.
   */
  it("reads each row once and formats nothing", () => {
    let idReads = 0;
    let valueReads = 0;
    const probe = (n: number): Row =>
      ({
        get id() {
          idReads += 1;
          return `J${n}`;
        },
        kind: "junction",
        cls: "point",
        context: "",
        get value() {
          valueReads += 1;
          return n;
        },
        format: null,
        canZoom: false,
      }) as unknown as Row;

    const rows = Array.from({ length: 100 }, (_, i) => probe(i));
    fitContent(rows, false);

    // Exactly one read of each field per row. Written as an equality
    // rather than a bound because this is the claim the whole feature
    // rests on: fitting a 50k list is one traversal, and a second scan or
    // a re-read would not show up anywhere else until someone noticed the
    // panel took a moment to resize.
    expect(idReads).toBe(rows.length);
    expect(valueReads).toBe(rows.length);
  });
});
