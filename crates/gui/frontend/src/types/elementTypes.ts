/**
 * Letter badge and colour per element type.
 *
 * The **letters** mirror each engine's `ELEMENT_KINDS` badge field — the
 * contract's §4.1 catalog is their source of truth, and a Rust test
 * (`frontend_badges_mirror_the_engine_catalogs`) fails the build when this
 * table drifts from it. The **colours** are this layer's own: presentation
 * the contract deliberately does not describe.
 *
 * Tags follow engineering convention: uppercase for physical assets
 * (OF outfall, SU storage unit, FD flow divider, RG rain gage, TK tank),
 * title case for non-spatial model objects (Cv, Pa) so a curve is never
 * read as a check valve.
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
  tank: { label: "TK", color: "#3daf75" },
  pipe: { label: "P", color: "#8a93a3" },
  pump: { label: "PU", color: "#d4a017" },
  valve: { label: "V", color: "#8a93a3" },
  // Urban drainage (uds). `junction` and `pump` are shared kind ids and
  // keep the entries above.
  outfall: { label: "OF", color: "#4a90d9" },
  storage: { label: "SU", color: "#3daf75" },
  divider: { label: "FD", color: "#8a93a3" },
  conduit: { label: "C", color: "#8a93a3" },
  orifice: { label: "OR", color: "#8a93a3" },
  weir: { label: "W", color: "#d4a017" },
  outlet: { label: "OL", color: "#8a93a3" },
  subcatchment: { label: "SC", color: "#3daf75" },
  raingage: { label: "RG", color: "#4a90d9" },
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
