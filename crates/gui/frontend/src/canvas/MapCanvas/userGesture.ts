/**
 * Whether the camera moved because someone moved it.
 *
 * The canvas moves its own camera constantly — framing a network, flying to
 * an element, re-fitting when a panel opens — and almost every question
 * downstream turns on telling those apart from a drag or a scroll. Whether
 * a framing may be replaced without asking, whether a fit should be
 * animated, whether a position is worth remembering: all of them mean "did
 * a person choose this".
 *
 * The two renderers report it differently and the rule was written out at
 * each of them. Same judgement, two spellings, no name — which is how one
 * of them comes to be extended and the other forgotten.
 */

/**
 * MapLibre: the compatibility event is present only when input drove the
 * move.
 *
 * `fitBounds`, `flyTo` and `jumpTo` all arrive without one, which is the
 * cleanest signal the library offers.
 */
export function mapMoveWasUserDriven(event: {
  originalEvent?: unknown;
}): boolean {
  return event.originalEvent != null;
}

/**
 * deck: the interaction state says which gesture is under way, if any.
 *
 * All four are checked rather than dragging alone — a scroll-zoom and a
 * rotate are choices as much as a pan is, and treating them as the app's
 * own movement would let them be overwritten.
 */
export function deckMoveWasUserDriven(interactionState?: {
  isDragging?: boolean;
  isPanning?: boolean;
  isZooming?: boolean;
  isRotating?: boolean;
}): boolean {
  return Boolean(
    interactionState?.isDragging ||
      interactionState?.isPanning ||
      interactionState?.isZooming ||
      interactionState?.isRotating,
  );
}
