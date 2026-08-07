/**
 * What the schematic's framing pass should do this time round.
 *
 * The pass runs whenever the view mode, the network, the layout space or
 * the couplings change, and it decides between four quite different things:
 * put the camera away, wait for something that has not arrived, frame the
 * network, or return to a camera it kept earlier. Those were nested
 * conditions inside an effect, reachable only by running the app — and
 * every camera defect this canvas has had lived among them.
 *
 * The decision is the whole of it; applying the plan is a few lines of deck
 * calls. Separating them means the part with the reasoning in it can be
 * read, named and tested, and the part that cannot be tested has nothing
 * left to get wrong.
 */

/**
 * The two layouts the schematic renderer draws.
 *
 * A plan is in the model's own coordinates and a topological layout is in
 * the grid the layout algorithm invents, so they are different spaces and a
 * camera from one lands nowhere in the other. Each keeps its own.
 */
export type SchematicSpace = "plan" | "topological";

export type FramingPlan =
  /**
   * Leaving the orthographic renderer. Keep the current camera under the
   * space it belonged to, so coming back returns to it rather than framing
   * the whole network again.
   */
  | { action: "stash"; space: SchematicSpace }
  /**
   * Nothing to do yet — the canvas is not on screen, or the layout is not
   * decided. Framing an empty layout would record it as framed and the real
   * one would never get its turn.
   */
  | { action: "wait" }
  | {
      action: "frame";
      /** The space being drawn now. */
      space: SchematicSpace;
      /** A different space was on screen; keep its camera before moving. */
      stashPrevious: SchematicSpace | null;
      /** Every kept camera describes a network that is no longer here. */
      discardSaved: boolean;
      /**
       * Use the camera kept for this space rather than framing the network.
       *
       * A kept camera is the record of having been here before, so it
       * answers both "have I framed this yet" and "where was I". Framing on
       * every arrival is what made switching views feel like it kept
       * pressing Fit network.
       */
      useSaved: boolean;
    };

export interface FramingInputs {
  viewMode: "map" | "schematic";
  /** Whether the canvas is the visible tab. */
  isActive: boolean;
  /** Whether the topological layout is the one being drawn. */
  topological: boolean;
  /** Whether the couplings that decide the topological layout have arrived. */
  couplingsResolved: boolean;
  /** The space currently on screen, or null before anything has been drawn. */
  currentSpace: SchematicSpace | null;
  /** The network the last framing pass ran against, by identity. */
  framedFor: { nodes: unknown; links: unknown } | null;
  nodes: unknown;
  links: unknown;
  /** Whether a camera is being kept for the space about to be drawn. */
  hasSavedCamera: boolean;
}

export function planFraming(input: FramingInputs): FramingPlan {
  if (input.viewMode !== "schematic") {
    return input.currentSpace
      ? { action: "stash", space: input.currentSpace }
      : { action: "wait" };
  }
  if (!input.isActive) return { action: "wait" };
  if (input.topological && !input.couplingsResolved) return { action: "wait" };

  const space: SchematicSpace = input.topological ? "topological" : "plan";
  const stashPrevious =
    input.currentSpace && input.currentSpace !== space
      ? input.currentSpace
      : null;

  // NOTE: a first pass, with nothing framed yet, counts as changed — so the
  // cameras are cleared before they can be used. That is what the code this
  // replaces did, and it is preserved here rather than corrected, because
  // this extraction is meant to change nothing. It is also the reason a
  // camera restored from storage never survived to be applied. Whether it
  // should stay is now a decision with a name and a test rather than an
  // expression nobody could reach.
  const discardSaved =
    input.framedFor?.nodes !== input.nodes ||
    input.framedFor?.links !== input.links;

  return {
    action: "frame",
    space,
    stashPrevious,
    discardSaved,
    useSaved: input.hasSavedCamera && !discardSaved,
  };
}
