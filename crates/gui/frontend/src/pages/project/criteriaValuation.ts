/** The wds saved-criteria shape as a criteria valuation (hydra-common
 * §7.3). The saved shape predates the criteria contract and the canvas
 * still reads it, so the store stays; this is the bridge. The backend
 * holds the same mapping (`wds_valuation_of` in `commands/report.rs`) —
 * a cross-boundary pair, tested on each side.
 *
 * The saved shape still has a `flow` band; the catalog no longer declares
 * that criterion, so sending it would be sending a key nothing reads. */

import type { ProjectCriteria } from "../../hooks";

export function wdsValuation(c: ProjectCriteria): Record<string, unknown> {
  return {
    minPressure: c.minPressureM,
    minResidual: c.minResidualMgL,
    maxAge: c.maxAgeH,
    pressure: [c.pressure.low, c.pressure.required, c.pressure.high],
    velocity: [c.velocity.low, c.velocity.target, c.velocity.high],
  };
}
