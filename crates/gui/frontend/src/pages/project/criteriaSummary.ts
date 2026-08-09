/** The criteria chip's read-only text: the whole ruler in one line, in
 * the active display system — minimum service pressure, then each band's
 * endpoints. Endpoints only: the chip is a reminder that criteria exist
 * and roughly where they sit; the popover it opens shows every field. */

import type { ProjectCriteria } from "../../hooks";
import {
  type Quantity,
  toDisplay,
  type UnitSystem,
  unitLabel,
} from "../../units";

function fmt(si: number, q: Quantity, sys: UnitSystem): string {
  const v = toDisplay(si, q, sys);
  const decimals = Math.abs(v) >= 100 ? 0 : Math.abs(v) >= 10 ? 1 : 2;
  return String(Number(v.toFixed(decimals)));
}

export function criteriaSummary(c: ProjectCriteria, sys: UnitSystem): string {
  const band = (q: Quantity, low: number, high: number) =>
    `${fmt(low, q, sys)}–${fmt(high, q, sys)} ${unitLabel(q, sys)}`;
  return [
    `≥ ${fmt(c.minPressureM, "pressure", sys)} ${unitLabel("pressure", sys)}`,
    `P ${band("pressure", c.pressure.low, c.pressure.high)}`,
    `V ${band("velocity", c.velocity.low, c.velocity.high)}`,
    `Q ${band("flow", c.flow.low, c.flow.high)}`,
  ].join("  ·  ");
}
