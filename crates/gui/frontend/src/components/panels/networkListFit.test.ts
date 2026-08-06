import { describe, expect, it } from "vitest";
import type { Row } from "./NetworkListRow";
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
   * The value lane is bounded by two numbers, so a caller formats those
   * rather than every row. A negative minimum matters: the minus sign is
   * a character the maximum does not have.
   */
  it("reports the extremes for the caller to format", () => {
    expect(fitContent([row({})], false, [-120.5, 42])?.extremes).toEqual([
      -120.5, 42,
    ]);
  });

  /**
   * The defect this shape exists to prevent: fitted to the values on
   * screen, the panel came undone as soon as the timeline moved. The
   * value lane never shrinks, so a wider number one step later took its
   * room from the id beside it — and ids began truncating in a panel that
   * had just been fitted.
   *
   * The extremes therefore come from the run's range, and the values in
   * the rows are not consulted at all.
   */
  it("ignores this period's values entirely", () => {
    const thisPeriod = [row({ value: 1 }), row({ value: 2 })];
    const another = [row({ value: -99999 }), row({ value: 88888 })];
    const range = [0, 500] as const;
    expect(fitContent(thisPeriod, false, range)?.extremes).toEqual([0, 500]);
    expect(fitContent(another, false, range)?.extremes).toEqual([0, 500]);
  });

  it("reports no extremes before a run declares a range", () => {
    expect(fitContent([row({}), row({})], false)?.extremes).toBeNull();
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
    // Zero, not one: the value lane is sized from the run's range, so the
    // rows' own values are never touched.
    expect(valueReads).toBe(0);
  });
});
