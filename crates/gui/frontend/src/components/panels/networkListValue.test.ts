import { describe, expect, it } from "vitest";
import { STATUS_LABELS } from "../../canvas/MapCanvas/colorUtils";
import type { Row } from "./NetworkList";
import { formatValue } from "./NetworkList";

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

const row = (value: number | null, format: Partial<Row["format"]>): Row =>
  ({ value, format: { key: "x", label: "X", ...format } }) as Row;

describe("a coded column", () => {
  /** The reported bug: the list and the inspector must agree. */
  it("prints the label, not the code", () => {
    expect(formatValue(row(3, { codes: STATUS_LABELS }), "si")).toBe("Open");
    expect(formatValue(row(2, { codes: STATUS_LABELS }), "si")).toBe("Closed");
  });

  /** Every code the engine can emit has a label, including the extended
   *  states that read as a qualified Open or Closed. */
  it("labels every status the engine emits", () => {
    for (const code of Object.keys(STATUS_LABELS).map(Number)) {
      const shown = formatValue(row(code, { codes: STATUS_LABELS }), "si");
      expect(shown).toBe(STATUS_LABELS[code]);
      expect(shown).not.toMatch(/^\d+$/);
    }
  });

  /**
   * A code this build does not name falls back to the number rather than
   * to a dash: "9" tells the user the engine reported something, where
   * "—" would claim it reported nothing.
   */
  it("shows an unknown code rather than hiding it", () => {
    expect(formatValue(row(99, { codes: STATUS_LABELS }), "si")).toBe("99");
  });

  /** Codes are not measurements, so the unit system cannot change them. */
  it("reads the same in either unit system", () => {
    const r = row(4, { codes: STATUS_LABELS });
    expect(formatValue(r, "us")).toBe(formatValue(r, "si"));
  });

  /** No value is still no value. */
  it("shows a dash when the row has no value", () => {
    expect(formatValue(row(null, { codes: STATUS_LABELS }), "si")).toBe("—");
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
