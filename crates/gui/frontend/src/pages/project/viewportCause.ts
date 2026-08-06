/**
 * What last moved the canvas, and the two questions that answers.
 *
 * This began as a boolean, `viewportUserOwned`, meaning "the camera is the
 * user's rather than the app's" — set by a drag, the zoom buttons, or a
 * fly-to, and cleared only by a fit. That was right for the question it
 * was asked: may an auto-fit reframe this view?
 *
 * It is the wrong shape for a second question. Following a relationship
 * from the inspector should reframe the canvas when the user is already
 * moving feature to feature, and leave it alone when they have panned
 * somewhere deliberately — and those two cases are the same boolean. So
 * the cause is recorded instead, and both questions are read from it.
 *
 * Keeping one value and two named readers, rather than two booleans set
 * side by side, is the point: two booleans drift the moment one call site
 * updates the one it remembers.
 */

/** What last moved the canvas. */
export type ViewportCause =
  /** The app framed the network — an initial load, or "Fit to view". */
  | "fit"
  /** The camera was flown to a specific element. */
  | "feature"
  /** The user moved it themselves: a drag, a scroll-zoom, the buttons. */
  | "user";

/**
 * Whether following a relationship should frame the element it lands on.
 *
 * True only after a fly-to, which makes the behaviour self-sustaining —
 * following flies, and flying is itself a `"feature"` move, so a chain of
 * hops keeps framing each element. It is also self-correcting: one pan
 * ends it, so a user who does not want it never has to know it exists.
 *
 * Biased toward not framing, deliberately. Declining when the user wanted
 * it costs them a click on "Zoom to"; framing when they did not costs them
 * a view they may have built on purpose.
 */
export function shouldZoomOnFollow(cause: ViewportCause): boolean {
  return cause === "feature";
}

/**
 * Whether the camera belongs to the user rather than the app.
 *
 * A fit hands framing back to the app; everything else — including a
 * fly-to — is a view worth preserving against an automatic refit.
 */
export function viewportIsUserOwned(cause: ViewportCause): boolean {
  return cause !== "fit";
}
