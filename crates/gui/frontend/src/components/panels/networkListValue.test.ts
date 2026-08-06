import { describe, expect, it } from "vitest";

import type { Row } from "./NetworkListRow";
import { formatValue, valueColor } from "./NetworkListRow";

/**
 * How the network list prints a value.
 *
 * Link status is an enumeration of OUT-file codes, and the list printed
 * the code: a row read "3" where the inspector beside it read "Open". The
 * list had no way to know status was not a measurement, so it fell
 * through to the dimensionless branch and formatted a number.
 *
 * The fix is that a coded column carries its own labels. These tests pin
 * that the list decodes nothing itself — the table travels with the
 * column, because a second copy of it is how the hover chip once came to
 * report every open link as "cv".
 */

/** A categorical column as an engine publishes one. */
const STATES: Record<number, { label: string; severity?: string }> = {
  2: { label: "Closed", severity: "alarm" },
  3: { label: "Open", severity: "nominal" },
  4: { label: "Active", severity: "caution" },
};

const row = (value: number | null, format: Partial<Row["format"]>): Row =>
  ({ value, format: { key: "x", label: "X", ...format } }) as Row;

describe("a coded column", () => {
  /** The reported bug: the list and the inspector must agree. */
  it("prints the label, not the code", () => {
    expect(formatValue(row(3, { codes: STATES }), "si")).toBe("Open");
    expect(formatValue(row(2, { codes: STATES }), "si")).toBe("Closed");
  });

  /** Every code the engine can emit has a label, including the extended
   *  states that read as a qualified Open or Closed. */
  it("labels every status the engine emits", () => {
    for (const code of Object.keys(STATES).map(Number)) {
      const shown = formatValue(row(code, { codes: STATES }), "si");
      expect(shown).toBe(STATES[code].label);
      expect(shown).not.toMatch(/^\d+$/);
    }
  });

  /**
   * A code this build does not name falls back to the number rather than
   * to a dash: "9" tells the user the engine reported something, where
   * "—" would claim it reported nothing.
   */
  it("shows an unknown code rather than hiding it", () => {
    expect(formatValue(row(99, { codes: STATES }), "si")).toBe("99");
  });

  /** Codes are not measurements, so the unit system cannot change them. */
  it("reads the same in either unit system", () => {
    const r = row(4, { codes: STATES });
    expect(formatValue(r, "us")).toBe(formatValue(r, "si"));
  });

  /** No value is still no value. */
  it("shows a dash when the row has no value", () => {
    expect(formatValue(row(null, { codes: STATES }), "si")).toBe("—");
  });
});

describe("an uncoded column", () => {
  /**
   * The branch status used to fall into. Left asserted so that giving
   * every column a code table by accident would be caught.
   */
  it("still prints a dimensionless number plainly", () => {
    expect(formatValue(row(0.2537, {}), "si")).toBe("0.25");
  });

  it("converts a column that names a quantity", () => {
    const metres = formatValue(row(10, { unit: "elevation" }), "si");
    const feet = formatValue(row(10, { unit: "elevation" }), "us");
    expect(metres).not.toBe(feet);
  });
});

describe("a coded value's colour", () => {
  /**
   * The engine judges each state — a closed link is an alarm, an active
   * valve worth noticing — and the list colours from that judgement
   * rather than from a ramp of its own, so a state reads the same in the
   * list, the legend and the canvas.
   */
  it("distinguishes the states the engine judged differently", () => {
    const closed = valueColor(row(2, { codes: STATES }));
    const open = valueColor(row(3, { codes: STATES }));
    const active = valueColor(row(4, { codes: STATES }));
    expect(new Set([closed, open, active]).size).toBe(3);
    for (const c of [closed, open, active]) expect(c).toMatch(/^rgb\(/);
  });

  /**
   * A state the engine passed no judgement on keeps the ordinary
   * foreground. A partition like a land-use class has no alarming member,
   * and colouring one would be the app asserting what the engine
   * declined to.
   */
  it("leaves an unjudged state in the ordinary colour", () => {
    const plain = { 1: { label: "Residential" } };
    expect(valueColor(row(1, { codes: plain }))).toBe("var(--text-secondary)");
  });

  /** Measured values are not states and keep the column's colour. */
  it("leaves a measured value alone", () => {
    expect(valueColor(row(12.5, { unit: "elevation" }))).toBe(
      "var(--text-secondary)",
    );
  });

  /** And a row with nothing reads as absent, not as a state. */
  it("dims a row with no value", () => {
    expect(valueColor(row(null, { codes: STATES }))).toBe(
      "var(--text-disabled)",
    );
  });
});
