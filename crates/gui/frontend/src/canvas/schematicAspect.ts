/**
 * Schematic layout aspect control (see `SchematicAspectSlider`).
 *
 * One slider trades the two spacings against each other: dragging up widens
 * layer spacing (X) while tightening spacing within a layer (Y), and dragging
 * down does the reverse. Node radii and link widths are layer properties and
 * are untouched — only the layout's proportions move.
 *
 * Why a ratio and not two independent spacings:
 *
 * - Scaling both axes equally is arithmetically indistinguishable from zooming.
 *   The gaps grow, and the only thing that could tell them apart is the
 *   elements' on-screen size, which at large-network zoom is pinned to
 *   `radiusMinPixels`. So the uniform component is not a control worth having;
 *   the viewport already has one.
 * - The camera fits `max(width, height)`, which divides the uniform component
 *   out on every change. Two independent sliders therefore shared a single
 *   visible degree of freedom — raising X was *identical* to lowering Y — and
 *   the pair read as one control that fought itself.
 *
 * Holding the product of the two scales at 1 leaves exactly the degree of
 * freedom that zoom cannot reach, and makes the fit harmless: there is no
 * uniform part left for it to remove, so the reshape survives being reframed
 * and the network never grows off-screen.
 */

/** Slider positions run 0–100 with the neutral ratio at the midpoint. */
export const ASPECT_SLIDER_MIN = 0;
export const ASPECT_SLIDER_MAX = 100;
export const ASPECT_SLIDER_DEFAULT = 50;

/**
 * Halving/doubling distance in slider units.
 *
 * At 25 the ends of the track are 2^±2, and because the two axes move in
 * opposite directions that is a **16× range of aspect ratios** end to end —
 * enough to take a tall thin spike to a wide flat fan and back.
 */
const UNITS_PER_DOUBLING = 25;

/**
 * Aspect factor for a slider position: >1 favours width, <1 favours height.
 * The midpoint is exactly 1.
 *
 * Geometric rather than linear: equal travel gives equal *ratio*, which is how
 * a scale control has to behave to feel even.
 */
export function aspectFactor(sliderValue: number): number {
  const clamped = clampSliderValue(sliderValue);
  return 2 ** ((clamped - ASPECT_SLIDER_DEFAULT) / UNITS_PER_DOUBLING);
}

/**
 * Per-axis spacing scales for a slider position.
 *
 * Their product is exactly 1, so the layout's area — and with it the network's
 * overall size on screen — is preserved while its proportions change. That is
 * what keeps this from behaving like a zoom.
 */
export function aspectScales(sliderValue: number): { x: number; y: number } {
  const root = Math.sqrt(aspectFactor(sliderValue));
  return { x: root, y: 1 / root };
}

/**
 * Coerce any stored or computed position into the track.
 *
 * Non-finite input resolves to the default rather than propagating: a NaN would
 * reach the layout as a NaN coordinate and blank the canvas, with the corrupt
 * value persisted so reopening the project would not recover.
 */
export function clampSliderValue(value: number): number {
  if (!Number.isFinite(value)) return ASPECT_SLIDER_DEFAULT;
  return Math.min(ASPECT_SLIDER_MAX, Math.max(ASPECT_SLIDER_MIN, value));
}

/**
 * Slider position for a pointer at `clientY` over a track spanning
 * `top`..`top + height`. Inverted against screen coordinates so dragging up
 * increases the value.
 */
export function sliderValueFromPointer(
  clientY: number,
  top: number,
  height: number,
): number {
  if (height <= 0) return ASPECT_SLIDER_DEFAULT;
  const fromTop = (clientY - top) / height;
  return clampSliderValue((1 - fromTop) * ASPECT_SLIDER_MAX);
}

/** Percentage from the bottom of the track, for positioning the thumb. */
export function thumbOffsetPercent(sliderValue: number): number {
  return clampSliderValue(sliderValue);
}
