/**
 * Whether the canvas camera moves or jumps, and for how long.
 *
 * Two gestures travel: fitting the whole network, and going to one element.
 * Both happen in two renderers — MapLibre for the geographic view, deck for
 * the diagram — and the rule has to be the same in all four places, which
 * is why it is here rather than at any of them.
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

/**
 * How long a flight to a single element lasts.
 *
 * Longer than a fit, because it is a different journey. A fit pulls back to
 * something already on screen; going to an element crosses the network to
 * land on one thing, often from far away, and the flight is what tells the
 * reader where that thing sits relative to where they were. Cut it and they
 * arrive somewhere with no idea how they got there.
 */
export const FLY_DURATION_MS = 800;

/**
 * How long to fly to an element, or zero to arrive at once.
 *
 * A duration rather than a flag because both renderers take one, and both
 * treat zero as "jump" — MapLibre through the `_ease` it runs every camera
 * move through, deck by declining to start a transition. One number, so
 * the two cannot end up disagreeing about what reduced motion means.
 *
 * MapLibre already honours the *operating system's* reduced-motion setting
 * on its own. This is the app's own switch, which a reader can turn on
 * without touching the OS, and nothing but this passes it on. Until it did,
 * asking for no motion still bought an 800ms swoop every time an element
 * was picked from the network list.
 */
export function flyDurationMs(reducedMotion: boolean): number {
  return reducedMotion ? 0 : FLY_DURATION_MS;
}
