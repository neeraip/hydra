/**
 * What a curve is *for*, in the user's words.
 *
 * The engine classifies every curve by how the model references it —
 * a pump's head curve, a tank's volume curve, a general-purpose valve's
 * headloss curve — and sends that as `CurveDto.kind`. The GUI used to
 * throw it away and show a point-count classification under the name
 * `curveType` instead, so the editor answered "what shape is this curve"
 * when the question on screen was "what is this curve".
 *
 * Roles are engine vocabulary and this map is presentation: an id the map
 * has never seen renders as itself rather than as nothing, so a curve role
 * added to the engine shows up as a slightly ugly label instead of a blank
 * line.
 *
 * That fallback is why `pump-volume` is deliberately absent. The engine
 * still has a `PumpVolume` curve kind, but nothing can produce one — it
 * names a curve type that exists in neither EPANET nor Hydra (a
 * constant-horsepower pump carries a power value and no curve), and it is
 * queued for removal at the next breaking release. Labelling it would
 * advertise a curve kind this GUI can never be shown.
 */

const ROLE_LABEL: Record<string, string> = {
  "pump-head": "Pump head",
  "pump-efficiency": "Pump efficiency",
  "tank-volume": "Tank volume",
  "gpv-headloss": "Valve headloss",
  "pcv-loss-ratio": "Valve loss ratio",
  // The engine's word for a curve the model references from nowhere. It
  // is reachable only by importing one; a curve created here is a pump
  // curve from the moment `create_curve` runs.
  generic: "Unassigned",
};

/** Display label for an engine curve role. */
export function curveRoleLabel(role: string): string {
  return ROLE_LABEL[role] ?? role;
}
