/**
 * The arithmetic behind the canvas's vertical sliders.
 *
 * Shared by the schematic aspect control and the node-size control, which
 * are the same object with different meanings: a 0–100 track, neutral at
 * the midpoint, dragged upward to increase.
 *
 * Hand-built rather than `<input type="range">` turned upright — see
 * `SchematicAspectSlider` for why a vertical range input is not portable
 * across the two webviews this app ships on.
 */

export const SLIDER_MIN = 0;
export const SLIDER_MAX = 100;
export const SLIDER_DEFAULT = 50;

/**
 * Coerce any stored or computed position into the track.
 *
 * Non-finite input resolves to the default rather than propagating: a NaN
 * would reach the layout as a NaN coordinate and blank the canvas, with the
 * corrupt value persisted so reopening the project would not recover.
 */
export function clampSliderValue(value: number): number {
  if (!Number.isFinite(value)) return SLIDER_DEFAULT;
  return Math.min(SLIDER_MAX, Math.max(SLIDER_MIN, value));
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
  if (height <= 0) return SLIDER_DEFAULT;
  const fromTop = (clientY - top) / height;
  return clampSliderValue((1 - fromTop) * SLIDER_MAX);
}

/** Percentage from the bottom of the track, for positioning the thumb. */
export function thumbOffsetPercent(sliderValue: number): number {
  return clampSliderValue(sliderValue);
}
