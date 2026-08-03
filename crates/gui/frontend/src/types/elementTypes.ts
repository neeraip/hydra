/**
 * Letter badge and colour per element type.
 *
 * Lives in `types/` because both sides of the app need it and neither may
 * import the other: `canvas/` never imports from `components/`, while
 * `components/` does import from `canvas/`. Adding the reverse edge for a
 * badge would close that loop, so the shared part — the letters and colours,
 * which must never disagree between surfaces — sits below both. Each side
 * renders it at whatever size suits: a row badge in the panels, a compact one
 * in the canvas hover chip.
 *
 * Pump is "Pu" because pipe already owns "P", and colour must not be the only
 * thing distinguishing two types.
 */

export interface ElementTypeBadge {
  label: string;
  color: string;
}

const BADGES: Record<string, ElementTypeBadge> = {
  // Water distribution (wds)
  junction: { label: "J", color: "#8a93a3" },
  reservoir: { label: "R", color: "#4a90d9" },
  tank: { label: "T", color: "#3daf75" },
  pipe: { label: "P", color: "#8a93a3" },
  pump: { label: "Pu", color: "#d4a017" },
  valve: { label: "V", color: "#8a93a3" },
  // Urban drainage (uds). Two-letter labels resolve the first-initial
  // collisions the fallback cannot (outfall/orifice/outlet all "O",
  // storage/subcatchment both "S"). `junction` and `pump` are shared kind
  // ids and keep the entries above.
  outfall: { label: "Of", color: "#4a90d9" },
  storage: { label: "St", color: "#3daf75" },
  divider: { label: "Dv", color: "#8a93a3" },
  conduit: { label: "C", color: "#8a93a3" },
  orifice: { label: "Or", color: "#8a93a3" },
  weir: { label: "W", color: "#d4a017" },
  outlet: { label: "Ou", color: "#8a93a3" },
  subcatchment: { label: "Sc", color: "#3daf75" },
  raingage: { label: "Rg", color: "#4a90d9" },
};

const FALLBACK_COLOR = "#8a93a3";

/** Badge for `type`, falling back to its initial for an unknown kind. */
export function elementTypeBadge(type: string): ElementTypeBadge {
  return (
    BADGES[type] ?? {
      label: type.charAt(0).toUpperCase() || "?",
      color: FALLBACK_COLOR,
    }
  );
}
