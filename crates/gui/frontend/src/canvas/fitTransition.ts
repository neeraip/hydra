/**
 * Whether a fit-to-network moves the camera or jumps it.
 *
 * Fitting happens for two different reasons and they want different
 * answers. When someone asks for it — the toolbar button, the palette
 * command, Cmd-0, or a panel opening and the map re-fitting around it — the
 * camera is going somewhere from somewhere, and animating that is what
 * says the two views are the same network. When the network has only just
 * arrived there is no "from": the camera is wherever the map happened to
 * initialise, so a flight from an arbitrary world view to the model is a
 * swoop that means nothing and delays the first frame worth looking at.
 *
 * And under reduced motion neither animates. That setting is not a
 * preference about polish; for some readers motion of this size is the
 * difference between using the app and not.
 */

/** Why the camera is being fitted. */
export type FitTrigger =
  /** Someone asked: a button, a shortcut, a command, a panel resizing. */
  | "request"
  /** The network has just loaded and the camera has nowhere meaningful to
   *  travel from. */
  | "load";

/**
 * How long the flight lasts.
 *
 * Long enough to read as one continuous movement rather than a cut, short
 * enough that someone who fits repeatedly while working is never waiting
 * for it. MapLibre's own default is derived from the distance travelled,
 * which on a network the width of a country is several seconds.
 */
export const FIT_DURATION_MS = 450;

/** Whether this fit should be animated. */
export function shouldAnimateFit(
  trigger: FitTrigger,
  reducedMotion: boolean,
): boolean {
  if (reducedMotion) return false;
  return trigger === "request";
}

/** An orthographic camera: where it looks and how close. */
export interface OrthoView {
  target: [number, number, number];
  zoom: number;
}

/**
 * Ease in and out, so the camera starts and stops rather than cuts.
 *
 * A linear tween of a camera reads as a mechanical slide: the eye notices
 * the instant it begins and the instant it stops, because nothing in the
 * world moves that way.
 */
export function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;
}

/**
 * The camera partway from one view to another.
 *
 * Zoom is interpolated in its own units rather than in scale. Zoom is
 * already logarithmic, so moving through it linearly is what makes the
 * apparent rate of approach steady — interpolating the scale factor
 * instead rushes the start and crawls at the end.
 */
export function interpolateOrtho(
  from: OrthoView,
  to: OrthoView,
  t: number,
): OrthoView {
  const e = easeInOutCubic(Math.min(1, Math.max(0, t)));
  const mix = (a: number, b: number) => a + (b - a) * e;
  return {
    target: [
      mix(from.target[0], to.target[0]),
      mix(from.target[1], to.target[1]),
      mix(from.target[2] ?? 0, to.target[2] ?? 0),
    ],
    zoom: mix(from.zoom, to.zoom),
  };
}
