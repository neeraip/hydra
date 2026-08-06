/**
 * How the scrubber's playhead moves.
 *
 * The fill and the handle are two drawings of one number, so they have to
 * move as one thing. They did not: the fill eased toward its new width over
 * 80ms while the handle had no transition at all, so the fill trailed the
 * handle every time the playhead moved — visibly during a drag, where the
 * handle is pinned to the cursor and the fill lags behind it, and during
 * playback, where the handle snaps to each step and the fill catches up
 * afterwards.
 *
 * The ease itself is worth keeping for playback, where it turns a sequence
 * of jumps into a glide. It is worth removing for a drag, where the
 * playhead is not animating toward anything — it is being placed, and any
 * ease at all is latency between the cursor and the thing it is holding.
 */

/** Duration and easing of the playhead's glide between steps. */
export const PLAYHEAD_EASE = "80ms linear";

/**
 * The `transition` for one of the playhead's two parts.
 *
 * @param property  the property that part animates — the fill grows by
 *                  `width`, the handle moves by `left`.
 * @param scrubbing whether the user is dragging the playhead right now.
 * @returns the CSS transition, or `undefined` to animate nothing.
 */
export function playheadTransition(
  property: "width" | "left",
  scrubbing: boolean,
): string | undefined {
  return scrubbing ? undefined : `${property} ${PLAYHEAD_EASE}`;
}
