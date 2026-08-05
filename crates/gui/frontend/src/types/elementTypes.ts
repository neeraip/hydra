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
 * read as a check valve. Colours are per element *class*, never per kind
 * — see the tint constants below.
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

/**
 * Badge tints, by element class.
 *
 * Muted by design and sharing nothing with the result/status vocabulary
 * (#4a90d9 sequential, #3daf75 good, #d4a017 caution, #c94040 excessive):
 * in a process display saturated colour should mean *something is
 * abnormal* (ISA-101 / High Performance HMI), so spending it on a pump or
 * a tank — which are simply present — competes with the colours that
 * carry meaning. The letters identify the kind; the tint only says which
 * class of thing it is, which the hover chip and inspector header need
 * because they have no tab to say it for them.
 */
const POINT_TINT = "#8a93a3";
const POLYLINE_TINT = "#a09a8c";
const REGION_TINT = "#8fa88f";
const COLLECTION_TINT = "#7f8794";

const BADGES: Record<string, ElementTypeBadge> = {
  // Water distribution (wds)
  junction: { label: "J", color: POINT_TINT },
  reservoir: { label: "R", color: POINT_TINT },
  tank: { label: "TK", color: POINT_TINT },
  pipe: { label: "P", color: POLYLINE_TINT },
  pump: { label: "PU", color: POLYLINE_TINT },
  valve: { label: "V", color: POLYLINE_TINT },
  // Urban drainage (uds). `junction` and `pump` are shared kind ids and
  // keep the entries above.
  outfall: { label: "OF", color: POINT_TINT },
  storage: { label: "SU", color: POINT_TINT },
  divider: { label: "FD", color: POINT_TINT },
  raingage: { label: "RG", color: POINT_TINT },
  conduit: { label: "C", color: POLYLINE_TINT },
  orifice: { label: "OR", color: POLYLINE_TINT },
  weir: { label: "W", color: POLYLINE_TINT },
  outlet: { label: "OL", color: POLYLINE_TINT },
  subcatchment: { label: "SC", color: REGION_TINT },
  // Non-spatial model objects (both engines).
  curve: { label: "Cv", color: COLLECTION_TINT },
  pattern: { label: "Pa", color: COLLECTION_TINT },
  control: { label: "Ct", color: COLLECTION_TINT },
  rule: { label: "Ru", color: COLLECTION_TINT },
  pollutant: { label: "Po", color: COLLECTION_TINT },
  timeseries: { label: "Ts", color: COLLECTION_TINT },
  // Drainage process parameter sets. Their letters match the `badge` the
  // engine declares for each kind — this map is a hand-kept copy of that,
  // and the six below were missing, so they fell through to the initial:
  // `landuse` and `lidcontrol` both rendered as a grey "L".
  landuse: { label: "Lu", color: COLLECTION_TINT },
  aquifer: { label: "Aq", color: COLLECTION_TINT },
  snowpack: { label: "Sn", color: COLLECTION_TINT },
  hydrograph: { label: "Uh", color: COLLECTION_TINT },
  lidcontrol: { label: "Li", color: COLLECTION_TINT },
  transect: { label: "Tr", color: COLLECTION_TINT },
};

const FALLBACK_COLOR = POINT_TINT;

/**
 * Letters published by the engines themselves, learned as their catalogs
 * are fetched.
 *
 * The map above is a hand-kept copy of something every engine already
 * declares (`ElementKind.badge`), and a copy is a thing that can fall
 * behind: six drainage kinds shipped with no entry and fell through to
 * their initial, so `landuse` and `lidcontrol` both rendered as "L".
 * Registering the catalog makes the engine's own letters authoritative
 * and the map a fallback for colour and for surfaces that render before
 * any catalog has loaded.
 */
const declaredLabels = new Map<string, string>();

export function registerElementBadges(
  kinds: readonly { id: string; badge: string }[],
): void {
  for (const k of kinds) {
    // Kind ids are shared across engines where the concept is shared
    // (`junction`, `pump`), and both engines badge those identically —
    // so registering a second catalog reinforces rather than overwrites.
    // Were they ever to disagree, the last engine loaded would win, and
    // the badges are meant to read the same everywhere.
    if (k.badge) declaredLabels.set(k.id, k.badge);
  }
}

/** Forget every registered catalog. For tests, which would otherwise
 * inherit whatever a previous one registered into this module-global. */
export function clearElementBadges(): void {
  declaredLabels.clear();
}

/**
 * Badge for `type`: the engine's declared letters where known, this
 * layer's colour, and the type's initial only when nothing has described
 * it at all.
 */
export function elementTypeBadge(type: string): ElementTypeBadge {
  const known = BADGES[type];
  const declared = declaredLabels.get(type);
  if (!known && !declared) {
    return {
      label: type.charAt(0).toUpperCase() || "?",
      color: FALLBACK_COLOR,
    };
  }
  return {
    label: declared ?? known?.label ?? "?",
    color: known?.color ?? COLLECTION_TINT,
  };
}
