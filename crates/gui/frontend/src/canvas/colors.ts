/**
 * Colour utilities shared between MapCanvas layers and inspector panels.
 * All functions return a CSS colour string (an `rgb(...)` value, or a
 * `var(...)` fallback for missing data).
 */

/** Threshold-based pressure colour (matches pressureRgba in MapCanvas/colorUtils). */
export function pressureColor(p: number): string {
  if (p < 24) return "rgb(201,64,64)";
  if (p < 35) return "rgb(212,160,23)";
  if (p < 45) return "rgb(61,175,117)";
  return "rgb(74,144,217)";
}

/**
 * Sequential ramp: blue (low) → cyan → green → yellow → orange/red (high).
 * Matches sequentialRgba in MapCanvas/colorUtils.
 */
export function sequentialColor(
  value: number,
  min: number,
  max: number,
): string {
  const range = max - min || 1;
  const t = Math.max(0, Math.min(1, (value - min) / range));
  let r: number, g: number, b: number;
  if (t < 0.25) {
    const s = t / 0.25;
    r = 0;
    g = Math.round(180 * s);
    b = Math.round(255 - 55 * s);
  } else if (t < 0.5) {
    const s = (t - 0.25) / 0.25;
    r = 0;
    g = Math.round(180 + 75 * s);
    b = Math.round(200 - 200 * s);
  } else if (t < 0.75) {
    const s = (t - 0.5) / 0.25;
    r = Math.round(255 * s);
    g = 255;
    b = 0;
  } else {
    const s = (t - 0.75) / 0.25;
    r = 255;
    g = Math.round(255 * (1 - s));
    b = 0;
  }
  return `rgb(${r},${g},${b})`;
}

/** Quality gradient: blue → teal → red. Matches qualityRgba in MapCanvas/colorUtils. */
export function qualityColor(value: number, min: number, max: number): string {
  const range = max - min || 1;
  const t = Math.max(0, Math.min(1, (value - min) / range));
  let r: number, g: number, b: number;
  if (t < 0.5) {
    const s = t * 2;
    r = Math.round(74 + s * (61 - 74));
    g = Math.round(144 + s * (175 - 144));
    b = Math.round(217 - s * (217 - 117));
  } else {
    const s = (t - 0.5) * 2;
    r = Math.round(61 + s * (201 - 61));
    g = Math.round(175 - s * (175 - 64));
    b = Math.round(117 - s * (117 - 64));
  }
  return `rgb(${r},${g},${b})`;
}

/** Flow magnitude colour (matches flowMagnitudeRgba in MapCanvas/colorUtils). */
export function flowColor(
  flow: number | null | undefined,
  maxFlow: number,
): string {
  if (flow == null) return "var(--text-primary)";
  const t = maxFlow > 0 ? Math.min(1, Math.abs(flow) / maxFlow) : 0;
  const r = Math.round(80 + 175 * t);
  const g = Math.round(200 - 120 * t);
  const b = Math.round(247 - 200 * t);
  return `rgb(${r},${g},${b})`;
}

/** Velocity colour (matches velocityRgba in MapCanvas/colorUtils). */
export function velocityColor(v: number): string {
  const t = Math.min(v / 1.5, 1);
  const r = Math.round(74 + t * (201 - 74));
  const g = Math.round(144 - t * (144 - 80));
  const b = Math.round(217 - t * (217 - 23));
  return `rgb(${r},${g},${b})`;
}

/**
 * Discrete status colour. Uses Hydra OUT-file codes (status_to_f32):
 * 0=XHead, 1=TempClosed, 2=Closed, 3=Open, 4=Active, 6=XFcv, 7=XPressure
 */
export function statusColor(status: number | null | undefined): string {
  if (status === 2 || status === 0 || status === 1) return "rgb(201,64,64)"; // closed variants — red
  if (status === 4 || status === 6 || status === 7) return "rgb(212,160,23)"; // active/controlled — amber
  return "rgb(120,150,185)"; // open (3) / unknown — blue-grey
}
