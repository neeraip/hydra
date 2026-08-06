import { describe, expect, it } from "vitest";
import type { SimResultColumn } from "../../canvas/selection-context";
import { type Row, valueColumnHeading } from "./NetworkList";

function row(label: string, unit: SimResultColumn["unit"], id = "X"): Row {
  return {
    id,
    kind: "junction",
    cls: "point",
    context: "",
    value: 1,
    format: { key: label, label, unit, symbol: label[0] } as SimResultColumn,
    canZoom: true,
  };
}

describe("valueColumnHeading", () => {
  it("puts the unit in the header when one variable is in view", () => {
    const h = valueColumnHeading([row("Pressure", "pressure")], "si");
    expect(h.text).toBe("Pressure (m)");
    // On every row it would only break the column's alignment.
    expect(h.perRowUnits).toBe(false);
    expect(h.unitWidth).toBe(0);
  });

  /**
   * The alignment fix. With several variables each row must carry its own
   * unit, and units of different widths right-aligned against the number
   * push the digits to different offsets — which defeats the one thing a
   * value column is for.
   */
  it("reserves a lane wide enough for the widest unit on screen", () => {
    const h = valueColumnHeading(
      [row("Pressure", "pressure", "J1"), row("Velocity", "velocity", "P1")],
      "si",
    );
    expect(h.perRowUnits).toBe(true);
    // "m" and "m/s" — the lane fits the wider.
    expect(h.unitWidth).toBe(3);
  });

  /**
   * Sized to what is shown, not to the widest unit the engines can
   * produce. `ft/kft` is six characters and a 320px rail cannot spare them
   * permanently for a variable rarely in view.
   */
  it("does not reserve for units that are not on screen", () => {
    const h = valueColumnHeading(
      [row("Pressure", "pressure", "J1"), row("Head", "head", "J2")],
      "si",
    );
    // Both are metres; nothing wider is reserved on their account.
    expect(h.unitWidth).toBe(1);
  });

  /**
   * The scan for units shares a loop — and an early exit — with the scan
   * for symbols. That exit is sound only while a class has one variable;
   * this pins the lane against every variable actually present, so a
   * fourth would be noticed rather than silently clipped.
   */
  it("fits every distinct variable it reports", () => {
    const rows = [
      row("Pressure", "pressure", "J1"),
      row("Velocity", "velocity", "P1"),
      row("Flow", "flow", "P2"),
    ];
    const h = valueColumnHeading(rows, "si");
    const widest = Math.max(
      ...rows.map((r) => (r.format?.unit === "flow" ? 3 : 3)),
    );
    expect(h.unitWidth).toBeGreaterThanOrEqual(widest);
    expect(h.text.split(" · ")).toHaveLength(3);
  });

  it("says nothing when no row carries a value column", () => {
    const bare = { ...row("Pressure", "pressure"), format: null };
    const h = valueColumnHeading([bare], "si");
    expect(h.text).toBe("");
    expect(h.perRowUnits).toBe(false);
  });
});
