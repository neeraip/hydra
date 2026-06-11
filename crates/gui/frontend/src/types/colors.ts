export const PRESSURE_THRESHOLD = 24;
export const PRESSURE_MIN = 21;
export const PRESSURE_MAX = 53;

export function pressureColor(p: number): string {
  if (p < PRESSURE_THRESHOLD) return "#c94040";
  if (p < 35) return "#d4a017";
  if (p < 45) return "#3daf75";
  return "#4a90d9";
}

// Delta color: positive = improvement (blue), negative = degradation (orange/red)
export function deltaColor(delta: number): string {
  if (delta > 6) return "#4a90d9";
  if (delta > 2) return "#7ab8e8";
  if (delta > 0) return "#a8d4f5";
  if (delta > -2) return "#d4a017";
  return "#c94040";
}
