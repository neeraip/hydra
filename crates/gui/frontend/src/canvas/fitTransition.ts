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
