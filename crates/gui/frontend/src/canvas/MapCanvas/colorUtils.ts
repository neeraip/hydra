import type { Link, Node } from "../../hooks";
import type { RampHint } from "../../hooks/results";
import { PRESSURE_THRESHOLD } from "../../types";
import type { LinkVariable, NodeVariable } from "../types";

export type RGBA = [number, number, number, number];

/**
 * Sequential ramps, one hue family per element class.
 *
 * Three classes can be coloured at once — a node variable, a link variable
 * and a catchment variable — and painted in one hue they left the reader
 * remembering which geometry meant which legend row. Hue now says *which
 * class*, lightness says *how much*: two channels, two questions.
 *
 * On the canvas that is a fair use of hue. The rule that hue carries state
 * rather than identity was written for chrome, where hue competed with
 * selection; here it is the data channel already, and nothing else is
 * asking for it. The families are chosen for what they describe — blue for
 * water level at a node, green for the land a catchment covers — and are
 * separated in OKLab chroma so no two are confusable at the same lightness.
 *
 * Every family is dark at the bottom of its range and bright at the top:
 * the ground is near-black, and the maxima are what these maps are read
 * for.
 */
type SeqFamily = readonly [
  readonly [number, number, number],
  readonly [number, number, number],
  readonly [number, number, number],
];

const SEQ_FAMILIES: Record<string, SeqFamily> = {
  /** Nodes: blue, for depth, head and water level. */
  point: [
    [12, 38, 78],
    [31, 122, 186],
    [186, 240, 253],
  ],
  /** Links: violet, distinct from both water blue and land green. */
  polyline: [
    [36, 17, 71],
    [109, 75, 196],
    [224, 208, 255],
  ],
  /** Catchments: green, for the surface they describe. */
  region: [
    [14, 44, 26],
    [47, 143, 82],
    [200, 245, 207],
  ],
};

/**
 * Diverging: bright teal ← dark centre → bright violet.
 *
 * Dark in the middle, not light. A diverging ramp is read for its extremes
 * — which way and how hard — and a light centre made "near zero" the
 * brightest thing on a dark canvas while strong flow in either direction
 * receded. The centre now sits close to the ground and recedes with it.
 *
 * Teal and violet rather than blue and red: this ramp shows direction, and
 * direction is not severity. Red belongs to the ramp that judges.
 *
 * The two ends are matched in perceptual lightness (OKLab L ≈ 0.75), so
 * neither direction looks stronger than the other at equal magnitude.
 *
 * Class-independent, unlike the sequential families: this ramp's shape —
 * bright, dark, bright — already marks it as a different kind of reading,
 * and splitting it three ways would need six more hues the palette does
 * not have room for.
 */
const DIV_LOW: [number, number, number] = [72, 198, 183];
const DIV_MID: [number, number, number] = [42, 47, 58];
const DIV_HIGH: [number, number, number] = [183, 155, 255];

/** A value that should exist and does not — an unreported element, a null
 * reading. Distinct from the network at rest, which has its own palette. */
export const NO_DATA_RGB: [number, number, number] = [110, 116, 126];

/** Stable hash of a string → float in [0, 1). Used for per-link phase offsets. */
export function hashStr(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++)
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  return (Math.abs(h) >>> 0) / 0x100000000;
}

/**
 * The network at rest, before any result exists.
 *
 * Differentiated by what a kind *does* (spec §4.3) rather than by what it
 * is called, so it works for an engine this file has never heard of. And
 * differentiated by lightness rather than hue, because hue is spent on
 * results: a reader glancing at an unsimulated model needs to see where the
 * system is fed and drained and where something acts on the flow, not to
 * learn a colour code.
 *
 * Everything used to paint one grey here, which said nothing at all.
 */
const BASE_CONVEYANCE_NODE: RGBA = [168, 180, 196, 220];
const BASE_BOUNDARY_NODE: RGBA = [232, 238, 246, 245];
const BASE_CONTROL_NODE: RGBA = [201, 209, 221, 235];
const BASE_CONVEYANCE_LINK: RGBA = [145, 158, 175, 210];
const BASE_CONTROL_LINK: RGBA = [206, 213, 223, 230];

/** A kind with no declared role is not in the flow network; it gets the
 * quietest treatment rather than a guess. */
export function baseNodeRgba(role: string | undefined): RGBA {
  if (role === "boundary") return BASE_BOUNDARY_NODE;
  if (role === "control") return BASE_CONTROL_NODE;
  return BASE_CONVEYANCE_NODE;
}

export function baseLinkRgba(role: string | undefined): RGBA {
  return role === "control" ? BASE_CONTROL_LINK : BASE_CONVEYANCE_LINK;
}

// ── Quality colour helper (shared between node quality and legacy use) ────────

export function qualityRgba(normalised: number, cls = "point"): RGBA {
  // A concentration, an age or a trace percentage is a magnitude, not a
  // verdict — so it takes the sequential ramp like every other magnitude.
  // It used to run blue → green → red, which said "high is bad" about a
  // quantity whose meaning depends entirely on the quality mode.
  return [...seqRgb(Math.max(0, Math.min(1, normalised)), cls), 230];
}

// ── Diverging comparison ramp (scenario Δ overlay) ────────────────────────────

// ── Node variable colour functions ────────────────────────────────────────────

export function pressureRgba(
  p: number,
  thresholds?: { low: number; required: number; high: number },
): RGBA {
  const low = thresholds?.low ?? PRESSURE_THRESHOLD;
  const req = thresholds?.required ?? 35;
  const high = thresholds?.high ?? 45;
  if (p < low) return [201, 64, 64, 255];
  if (p < req) return [212, 160, 23, 255];
  if (p < high) return [61, 175, 117, 255];
  return [74, 144, 217, 255];
}

/**
 * Sequential ramp: single-hue light → dark blue over [min, max].
 *
 * This replaced a blue→cyan→green→yellow→red rainbow. A rainbow is not
 * ordered by lightness, so it invents boundaries the data does not have,
 * ranks badly under colour-vision deficiency, and — the reason it mattered
 * here — spent green and red, which the banded ramp needs for judgements.
 * One hue, monotonic in lightness, reads as a magnitude and nothing else.
 *
 * The same ramp the engine-generic path uses, so a head map and a depth map
 * are read the same way.
 */
export function sequentialRgba(
  value: number | null | undefined,
  min: number,
  max: number,
  alpha = 220,
  cls = "point",
): RGBA {
  if (value == null) return [...NO_DATA_RGB, alpha];
  const range = max - min || 1;
  const t = Math.max(0, Math.min(1, (value - min) / range));
  return [...seqRgb(t, cls), alpha];
}

/** The sequential ramp at `t` for one class, through its midpoint. Two
 * segments give each family its chroma arc — dark, through a saturated
 * mid, to near-white — which one straight line between the ends could not.
 * An unknown class falls back to the node family. */
export function seqRgb(t: number, cls = "point"): [number, number, number] {
  const [low, mid, high] = SEQ_FAMILIES[cls] ?? SEQ_FAMILIES.point;
  return t < 0.5 ? blend(low, mid, t / 0.5) : blend(mid, high, (t - 0.5) / 0.5);
}

/** Pick node RGBA based on the active node variable. Non-junctions always use their type colour. */
export function nodeRgba(
  node: Node & { position: [number, number] },
  nodeVar: NodeVariable,
  headMin: number,
  headMax: number,
  demandMin: number,
  demandMax: number,
  qualityMin: number,
  qualityMax: number,
  pressureThresh?: { low: number; required: number; high: number },
  /** The kind's declared role (spec §4.3), for kinds that carry no value
   * of the active variable and so show the network-at-rest palette. */
  role?: string,
): RGBA {
  if (node.type !== "junction") return baseNodeRgba(role);
  switch (nodeVar) {
    case "pressure":
      return node.pressure != null
        ? pressureRgba(node.pressure, pressureThresh)
        : [...NO_DATA_RGB, 190];
    case "head":
      return sequentialRgba(node.head, headMin, headMax);
    case "demand":
      return sequentialRgba(node.demand, demandMin, demandMax);
    case "quality":
      if (node.quality != null) {
        const range = qualityMax - qualityMin || 1;
        return qualityRgba((node.quality - qualityMin) / range);
      }
      return [...NO_DATA_RGB, 190];
  }
}

// ── Link variable colour functions ────────────────────────────────────────────

/**
 * Colour for an element whose value is unknown, as distinct from zero.
 *
 * Several ramps already fall back to this grey for a null value; it is named
 * here because the whole network uses it before a simulation exists. The
 * distinction matters most for velocity, whose ramp takes a non-nullable
 * number — an unsimulated link would otherwise render the definite colour of
 * "0 m/s" for a value nobody has computed.
 */
export const NO_RESULT_RGBA: RGBA = [100, 100, 100, 200];

// ── Engine-generic ramps (catalog-driven results) ─────────────────────────────

/** Linear blend between two RGB triples at `t` ∈ [0, 1]. */
// ── Perceptual interpolation ────────────────────────────────────────────────
//
// Ramps are mixed in OKLab, not in sRGB. Mixing raw channel values makes
// equal steps in the data unequal to the eye — the middle of a ramp bunches
// and its ends stretch — so a reader mis-ranks values by an amount that
// depends on where in the range they fall. OKLab is near-uniform, so an
// equal step in the data is an equal step in appearance.

function srgbToLinear(c: number): number {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(v: number): number {
  const c = v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055;
  return Math.max(0, Math.min(255, Math.round(c * 255)));
}

type Oklab = [number, number, number];

function toOklab([r, g, b]: readonly [number, number, number]): Oklab {
  const lr = srgbToLinear(r);
  const lg = srgbToLinear(g);
  const lb = srgbToLinear(b);
  const l = Math.cbrt(
    0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb,
  );
  const m = Math.cbrt(
    0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb,
  );
  const s = Math.cbrt(
    0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb,
  );
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function fromOklab([L, a, b]: Oklab): [number, number, number] {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    linearToSrgb(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    linearToSrgb(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    linearToSrgb(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

function blend(
  a: readonly [number, number, number],
  b: readonly [number, number, number],
  t: number,
): [number, number, number] {
  const A = toOklab(a);
  const B = toOklab(b);
  return fromOklab([
    A[0] + t * (B[0] - A[0]),
    A[1] + t * (B[1] - A[1]),
    A[2] + t * (B[2] - A[2]),
  ]);
}

/**
 * Banded ramp, within criteria → exceeding: quiet and dark, then warming
 * and brightening.
 *
 * The only ramp that passes judgement, so it is the only one spending warm
 * hue. It carries that judgement in *lightness* as well: each band is
 * strictly brighter than the one before, so it survives greyscale and
 * colour-vision deficiency, and so the worst band is the most prominent
 * thing on the canvas rather than the darkest.
 *
 * The old bands ran green → amber → red, which is the worst pairing for
 * colour-vision deficiency and was not even ordered — its second step was
 * lighter than its third. Normal is now simply quiet, in the manner of a
 * process display, rather than a green that competes for attention with
 * everything else on screen.
 */
export const BAND_STEPS: readonly [number, number, number][] = [
  [38, 52, 66],
  [92, 84, 62],
  [156, 116, 44],
  [214, 132, 48],
  [255, 168, 96],
];

/**
 * Qualitative palette for `categorical` states the engine passes no
 * judgement on, indexed by position in its declared item list.
 *
 * These are a closed set of *states*, not magnitudes, so this palette must
 * not read as ordered: every entry sits at a similar lightness and differs
 * by hue, the opposite of the sequential and banded ramps. The hues are
 * spaced around the wheel and avoid the pure red/green pairing that
 * colour-vision deficiency collapses.
 */
export const CATEGORY_STEPS: readonly [number, number, number][] = [
  [120, 150, 185],
  [212, 160, 23],
  [176, 106, 92],
  [110, 160, 140],
  [150, 128, 190],
  [180, 168, 96],
];

/**
 * States the engine *has* judged (§6.1 severity), which is a different
 * reading and gets a different treatment: not "which state is this?" but
 * "is anything wrong here?".
 *
 * Graded by prominence rather than by hue alone, so the answer survives a
 * glance at a whole network: nominal recedes toward the quiet blue-grey the
 * network-at-rest palette uses, caution is the amber the banded ramp
 * spends on its upper steps, and alarm is the one saturated red on the
 * canvas. Ordered in lightness as well as hue, so it survives greyscale and
 * colour-vision deficiency — the same discipline as the banded ramp, which
 * this is the discrete counterpart of.
 */
export const SEVERITY_RGB: Record<string, [number, number, number]> = {
  nominal: [120, 150, 185],
  caution: [212, 160, 23],
  alarm: [201, 64, 64],
};

/**
 * Colour for one declared state: its severity when the engine gave one,
 * otherwise its position in the qualitative palette.
 *
 * Severity wins because it is the stronger claim. An engine that says a
 * state is an alarm has told us something a position never could, and
 * ignoring it would paint a closed pipe as merely the third kind of link.
 */
export function categoryRgba(
  index: number,
  alpha = 220,
  severity?: string,
): RGBA {
  const judged = severity ? SEVERITY_RGB[severity] : undefined;
  if (judged) return [...judged, alpha];
  // Wrap rather than clamp: two states sharing a colour is confusing, but
  // a run of trailing states sharing the *last* colour is worse.
  const c =
    CATEGORY_STEPS[
      ((index % CATEGORY_STEPS.length) + CATEGORY_STEPS.length) %
        CATEGORY_STEPS.length
    ];
  return [...c, alpha];
}

/**
 * Colour for one engine-generic value against its variable's per-run range
 * and ramp hint — the catalog-driven counterpart of the per-variable wds
 * ramps above, with zero engine knowledge:
 *
 * - `sequential`: single-hue light → dark blue over [min, max];
 * - `diverging`: teal ← neutral → violet, centred on zero, scaled by the
 *   larger magnitude side (flow direction reads at a glance);
 * - `banded`: five discrete good → excessive steps over [min, max];
 * - `categorical`: one qualitative colour per engine-declared state.
 *
 * Non-finite values (unreported elements) render the shared no-result grey.
 */
export function genericRgba(
  value: number | null | undefined,
  variable: { min: number; max: number; ramp: RampHint },
  alpha = 220,
  /** Element class, selecting the sequential hue family. Diverging and
   * banded are class-independent — see their own notes. */
  cls = "point",
): RGBA {
  if (value == null || !Number.isFinite(value)) return NO_RESULT_RGBA;
  const { min, max, ramp } = variable;
  if (ramp.type === "categorical") {
    const i = ramp.items.findIndex((it) => it.value === value);
    // A state the engine did not declare is not a state we can name, so it
    // renders as absent rather than borrowing another state's colour.
    return i < 0
      ? NO_RESULT_RGBA
      : categoryRgba(i, alpha, ramp.items[i].severity);
  }
  if (ramp.type === "diverging") {
    const scale = Math.max(Math.abs(min), Math.abs(max));
    const t = scale > 0 ? Math.max(-1, Math.min(1, value / scale)) : 0;
    const rgb =
      t < 0 ? blend(DIV_MID, DIV_LOW, -t) : blend(DIV_MID, DIV_HIGH, t);
    return [...rgb, alpha];
  }
  const span = max - min;
  const t = span > 0 ? Math.max(0, Math.min(1, (value - min) / span)) : 0;
  if (ramp.type === "banded") {
    const step = Math.min(
      BAND_STEPS.length - 1,
      Math.floor(t * BAND_STEPS.length),
    );
    return [...BAND_STEPS[step], alpha];
  }
  return [...seqRgb(t, cls), alpha];
}

export function velocityRgba(
  v: number,
  thresholds?: { low: number; target: number; high: number },
): RGBA {
  // Thresholded velocity is a judgement, so it speaks the banded ramp's
  // language rather than a second warm palette of its own.
  if (thresholds) {
    if (v < thresholds.low) return [...BAND_STEPS[0], 220];
    if (v < thresholds.target) return [...BAND_STEPS[2], 220];
    if (v < thresholds.high) return [...BAND_STEPS[3], 220];
    return [...BAND_STEPS[4], 220];
  }
  // Untresholded velocity is a magnitude, so it takes the sequential ramp
  // — the same one head, depth and every generic magnitude uses.
  return [...seqRgb(Math.min(v / 1.5, 1), "polyline"), 220];
}

/** Flow magnitude on the sequential ramp; the banded ramp where the user
 * has set thresholds, because thresholds make it a judgement. */
export function flowMagnitudeRgba(
  flow: number | null | undefined,
  maxFlow: number,
  alpha = 200,
  thresholds?: { low: number; target: number; high: number },
): RGBA {
  if (flow == null) return [...NO_DATA_RGB, alpha];
  if (thresholds) {
    const abs = Math.abs(flow);
    if (abs < thresholds.low) return [...BAND_STEPS[0], alpha];
    if (abs < thresholds.target) return [...BAND_STEPS[2], alpha];
    if (abs < thresholds.high) return [...BAND_STEPS[3], alpha];
    return [...BAND_STEPS[4], alpha];
  }
  const t = maxFlow > 0 ? Math.min(1, Math.abs(flow) / maxFlow) : 0;
  return [...seqRgb(t, "polyline"), alpha];
}

/**
 * Status RGBA using Hydra OUT-file codes (status_to_f32):
 * 0=XHead, 1=TempClosed, 2=Closed, 3=Open, 4=Active, 6=XFcv, 7=XPressure
 */
/**
 * Human label for Hydra OUT-file link status codes (`status_to_f32` in the
 * engine's out writer): 0=XHead, 1=TempClosed, 2=Closed, 3=Open, 4=Active,
 * 6=XFcv, 7=XPressure.
 *
 * Lives beside `statusRgba` because both decode the same table, and a second
 * copy of it elsewhere is how the hover chip came to report every open link
 * as "cv".
 */
export function statusLabel(s: number | null | undefined): string {
  if (s === 3) return "Open";
  if (s === 2) return "Closed";
  if (s === 4) return "Active";
  if (s === 0) return "Closed (XHead)";
  if (s === 1) return "Temp Closed";
  if (s === 6) return "Active (XFcv)";
  if (s === 7) return "Active (XPressure)";
  return "—";
}

/**
 * Link status on the canvas, coloured by how remarkable the state is.
 *
 * Drawn from the same severity table the catalog-driven legend uses, so the
 * swatch a reader looks up always matches the link they looked it up for.
 * The code→severity mapping mirrors the engine's own published catalog
 * (§6.1); it is restated here only because this path colours from the fixed
 * wds period arrays rather than from the catalog payload.
 */
export function statusRgba(status: number | null | undefined): RGBA {
  // Closed variants: 2 closed, 0 excess head, 1 temporarily closed.
  if (status === 2 || status === 0 || status === 1) {
    return [...SEVERITY_RGB.alarm, 200];
  }
  // Controlled variants: 4 active, 6 setpoint not met, 7 excess pressure.
  if (status === 4 || status === 6 || status === 7) {
    return [...SEVERITY_RGB.caution, 200];
  }
  return [...SEVERITY_RGB.nominal, 180]; // open (3) / unknown
}

/**
 * Fixed upper bound (per-unit headloss, m/km) for the link headloss ramp.
 * Mirrors velocity's fixed 1.5 m/s cap rather than a per-period rescale so
 * colours stay comparable while scrubbing the timeline; typical design
 * guidance treats ≥ 10 m/km as excessive.
 */
export const LINK_HEADLOSS_MAX = 10;

/** Headloss: grey (no data) → sequential blue → red ramp capped at
 * {@link LINK_HEADLOSS_MAX}. */
export function headlossRgba(headloss: number | null | undefined): RGBA {
  if (headloss == null) return [...NO_DATA_RGB, 200];
  return sequentialRgba(
    Math.abs(headloss),
    0,
    LINK_HEADLOSS_MAX,
    220,
    "polyline",
  );
}

/** Link quality: grey (no data) → the node quality ramp normalised to the
 * result's quality range. */
export function linkQualityRgba(
  quality: number | null | undefined,
  qualityMin: number,
  qualityMax: number,
): RGBA {
  if (quality == null) return [...NO_DATA_RGB, 200];
  const range = qualityMax - qualityMin || 1;
  return qualityRgba((quality - qualityMin) / range, "polyline");
}

/** Pick link RGBA based on the active link variable. Pumps always use their fixed colour. */
export function linkRgba(
  link: Link,
  linkVar: LinkVariable,
  flowMax: number,
  velocityThresh?: { low: number; target: number; high: number },
  flowThresh?: { low: number; target: number; high: number },
  qualityMin = 0,
  qualityMax = 1,
): RGBA {
  switch (linkVar) {
    case "flow":
      return flowMagnitudeRgba(link.flow, flowMax, 200, flowThresh);
    case "velocity":
      // Absent (engine served no velocity) is unknown, not 0 m/s.
      return link.velocity == null
        ? NO_RESULT_RGBA
        : velocityRgba(link.velocity, velocityThresh);
    case "status":
      return statusRgba(link.status);
    case "headloss":
      return headlossRgba(link.headloss);
    case "quality":
      return linkQualityRgba(link.quality, qualityMin, qualityMax);
  }
}

// ── Legend gradients ────────────────────────────────────────────────────────
//
// Derived from the ramp functions above, never hand-written beside them. The
// legend used to carry its own CSS copies of every palette, which is how it
// came to disagree with the map about what "open" looked like — a comment
// insisting the two must match is not a mechanism that makes them match.

const css = (c: readonly [number, number, number]) =>
  `rgb(${c[0]},${c[1]},${c[2]})`;

/** The sequential ramp as a CSS gradient, sampled from the ramp itself. */
export function sequentialGradientCss(cls = "point"): string {
  const stops = Array.from({ length: 9 }, (_, i) => {
    const t = i / 8;
    return `${css(seqRgb(t, cls))} ${Math.round(t * 100)}%`;
  });
  return `linear-gradient(to right, ${stops.join(", ")})`;
}

/** The diverging ramp as a CSS gradient, negative → zero → positive. */
export function divergingGradientCss(): string {
  const stops = Array.from({ length: 9 }, (_, i) => {
    const t = i / 8;
    const value = -1 + 2 * t;
    const c = genericRgba(value, {
      min: -1,
      max: 1,
      ramp: { type: "diverging" },
    });
    return `${css([c[0], c[1], c[2]])} ${Math.round(t * 100)}%`;
  });
  return `linear-gradient(to right, ${stops.join(", ")})`;
}

/** The banded ramp as hard steps, one per band. */
export function bandedGradientCss(): string {
  const stops = BAND_STEPS.map(
    (c, i) =>
      `${css(c)} ${(i * 100) / BAND_STEPS.length}% ${((i + 1) * 100) / BAND_STEPS.length}%`,
  );
  return `linear-gradient(to right, ${stops.join(", ")})`;
}
