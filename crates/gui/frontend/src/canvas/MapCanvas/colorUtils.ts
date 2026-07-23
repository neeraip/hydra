import type { Link, Node } from "../../hooks";
import { PRESSURE_THRESHOLD } from "../../types";
import type { LinkVariable, NodeVariable } from "../types";

export type RGBA = [number, number, number, number];

/** Stable hash of a string → float in [0, 1). Used for per-link phase offsets. */
export function hashStr(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++)
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  return (Math.abs(h) >>> 0) / 0x100000000;
}

export function nodeTypeRgba(type: string): RGBA {
  if (type === "reservoir") return [74, 144, 217, 255];
  if (type === "tank") return [61, 175, 117, 255];
  return [180, 195, 215, 220]; // junction — muted steel blue-grey
}

// ── Quality colour helper (shared between node quality and legacy use) ────────

export function qualityRgba(normalised: number): RGBA {
  const t = Math.max(0, Math.min(1, normalised));
  if (t < 0.5) {
    const s = t * 2;
    return [
      Math.round(74 + s * (61 - 74)),
      Math.round(144 + s * (175 - 144)),
      Math.round(217 - s * (217 - 117)),
      230,
    ];
  }
  const s = (t - 0.5) * 2;
  return [
    Math.round(61 + s * (201 - 61)),
    Math.round(175 - s * (175 - 64)),
    Math.round(117 - s * (117 - 64)),
    230,
  ];
}

// ── Diverging comparison ramp (scenario Δ overlay) ────────────────────────────

/**
 * Diverging ramp for scenario-comparison deltas: blue (below baseline) →
 * neutral grey at 0 → red (above baseline). `delta / maxAbs` is clamped to
 * [-1, 1]; non-finite / missing deltas render the "no data" grey used by the
 * other link ramps. Callers floor `maxAbs` (see compare.ts MIN_MAX_ABS) —
 * a non-positive value here falls back to 1 rather than dividing by zero.
 */
export function divergingRgba(
  delta: number | null | undefined,
  maxAbs: number,
  alpha = 230,
): RGBA {
  if (delta == null || !Number.isFinite(delta)) return [100, 100, 100, alpha];
  const t = Math.max(-1, Math.min(1, delta / (maxAbs > 0 ? maxAbs : 1)));
  // Endpoints match the app palette: blue #4a90d9, red #c94040; grey centre.
  const end: [number, number, number] = t < 0 ? [74, 144, 217] : [201, 64, 64];
  const s = Math.abs(t);
  return [
    Math.round(150 + s * (end[0] - 150)),
    Math.round(150 + s * (end[1] - 150)),
    Math.round(150 + s * (end[2] - 150)),
    alpha,
  ];
}

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
 * Sequential ramp: blue (low) → green → yellow → orange (high).
 * Used for head, demand, and quality node variables.
 */
export function sequentialRgba(
  value: number | null | undefined,
  min: number,
  max: number,
  alpha = 220,
): RGBA {
  if (value == null) return [100, 100, 100, alpha];
  const range = max - min || 1;
  const t = Math.max(0, Math.min(1, (value - min) / range));
  if (t < 0.25) {
    const s = t / 0.25;
    return [0, Math.round(180 * s), Math.round(255 - 55 * s), alpha];
  }
  if (t < 0.5) {
    const s = (t - 0.25) / 0.25;
    return [0, Math.round(180 + 75 * s), Math.round(200 - 200 * s), alpha];
  }
  if (t < 0.75) {
    const s = (t - 0.5) / 0.25;
    return [Math.round(255 * s), 255, 0, alpha];
  }
  const s = (t - 0.75) / 0.25;
  return [255, Math.round(255 * (1 - s)), 0, alpha];
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
): RGBA {
  if (node.type !== "junction") return nodeTypeRgba(node.type);
  switch (nodeVar) {
    case "pressure":
      return node.pressure != null
        ? pressureRgba(node.pressure, pressureThresh)
        : [180, 195, 215, 180];
    case "head":
      return sequentialRgba(node.head, headMin, headMax);
    case "demand":
      return sequentialRgba(node.demand, demandMin, demandMax);
    case "quality":
      if (node.quality != null) {
        const range = qualityMax - qualityMin || 1;
        return qualityRgba((node.quality - qualityMin) / range);
      }
      return [180, 195, 215, 180];
  }
}

// ── Link variable colour functions ────────────────────────────────────────────

export function velocityRgba(
  v: number,
  thresholds?: { low: number; target: number; high: number },
): RGBA {
  if (thresholds) {
    if (v < thresholds.low) return [61, 175, 117, 220]; // below low  — good
    if (v < thresholds.target) return [212, 160, 23, 220]; // low–target — moderate
    if (v < thresholds.high) return [201, 120, 64, 220]; // target–high — elevated
    return [201, 64, 64, 220]; // above high  — excessive
  }
  const t = Math.min(v / 1.5, 1);
  return [
    Math.round(74 + t * (201 - 74)),
    Math.round(144 - t * (144 - 80)),
    Math.round(217 - t * (217 - 23)),
    220,
  ];
}

/** Flow magnitude: grey (no data) → cyan (low) → orange (max). */
export function flowMagnitudeRgba(
  flow: number | null | undefined,
  maxFlow: number,
  alpha = 200,
  thresholds?: { low: number; target: number; high: number },
): RGBA {
  if (flow == null) return [100, 100, 100, alpha];
  if (thresholds) {
    const abs = Math.abs(flow);
    if (abs < thresholds.low) return [61, 175, 117, alpha]; // below low  — good
    if (abs < thresholds.target) return [212, 160, 23, alpha]; // low–target — moderate
    if (abs < thresholds.high) return [201, 120, 64, alpha]; // target–high — elevated
    return [201, 64, 64, alpha]; // above high  — excessive
  }
  const t = maxFlow > 0 ? Math.min(1, Math.abs(flow) / maxFlow) : 0;
  return [
    Math.round(80 + 175 * t),
    Math.round(200 - 120 * t),
    Math.round(247 - 200 * t),
    alpha,
  ];
}

/**
 * Status RGBA using Hydra OUT-file codes (status_to_f32):
 * 0=XHead, 1=TempClosed, 2=Closed, 3=Open, 4=Active, 6=XFcv, 7=XPressure
 */
export function statusRgba(status: number | null | undefined): RGBA {
  if (status === 2 || status === 0 || status === 1) return [201, 64, 64, 200]; // closed variants — red
  if (status === 4 || status === 6 || status === 7) return [212, 160, 23, 200]; // active/controlled — amber
  return [120, 150, 185, 180]; // open (3) / unknown — blue-grey
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
  if (headloss == null) return [100, 100, 100, 200];
  return sequentialRgba(Math.abs(headloss), 0, LINK_HEADLOSS_MAX);
}

/** Link quality: grey (no data) → the node quality ramp normalised to the
 * result's quality range. */
export function linkQualityRgba(
  quality: number | null | undefined,
  qualityMin: number,
  qualityMax: number,
): RGBA {
  if (quality == null) return [100, 100, 100, 200];
  const range = qualityMax - qualityMin || 1;
  return qualityRgba((quality - qualityMin) / range);
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
  if (link.type === "pump") return [212, 160, 23, 220];
  switch (linkVar) {
    case "flow":
      return flowMagnitudeRgba(link.flow, flowMax, 200, flowThresh);
    case "velocity":
      return velocityRgba(link.velocity, velocityThresh);
    case "status":
      return statusRgba(link.status);
    case "headloss":
      return headlossRgba(link.headloss);
    case "quality":
      return linkQualityRgba(link.quality, qualityMin, qualityMax);
  }
}
