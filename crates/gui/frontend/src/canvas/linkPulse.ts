/**
 * Which link variables the flow pulse applies to, and what it means for each.
 *
 * The pulse was built for Flow and Velocity, where motion and colour say the
 * same thing. It never actually read the coloured variable, though — speed
 * and direction come from the link's own flow and velocity, whatever is
 * being coloured — so the restriction was a policy about when to show it
 * rather than a limit on what could be shown, and the toggle sat inert on
 * the other selections.
 *
 * Every selection has an honest animation now, and they are not all the
 * same animation: what differs is not the motion but what the motion is
 * claiming.
 */

import { LINK_VARIABLES } from "./canvasVariables";
import type { LinkVariable } from "./types";

/**
 * What motion is being asked to say.
 *
 * - `magnitude` — how fast, and which way. The colour is already a rate, so
 *   a pulse that keeps pace with it reinforces rather than competes.
 * - `transport` — something is being carried along, at the water's speed.
 *   The colour is a property of the water's content rather than of its
 *   movement, so the two are free to disagree: a pipe can crawl with high
 *   chlorine or race with none. Sharing the rate pattern would teach the
 *   reader that a fast pulse means a high reading, which is true of the
 *   rates and false here.
 * - `presence` — whether the link is carrying water at all, and which way.
 *   One rate for every moving link, because the legend beside it has no
 *   scale for motion to borrow, and a varying speed would invent one.
 * - `none` — nothing truthful to show.
 */
export type PulseKind = "magnitude" | "transport" | "presence" | "none";

/**
 * The pulse rate every moving link shares under `presence`.
 *
 * Mid-range: fast enough to read as deliberate at a glance, slow enough
 * that a screen full of them is not agitating.
 */
const PRESENCE_SPEED = 0.45;

/** Velocity, in m/s, that the pulse reaches full rate at. */
const FULL_RATE_VELOCITY = 1.5;

/** The rate for a link the results do not describe. */
const UNKNOWN_SPEED = 0.2;

/**
 * How this variable should animate.
 *
 * Unit headloss gets the flow pulse unchanged. Head is only lost in the
 * direction the water moves, so motion and colour cannot disagree here the
 * way they could on an unrelated quantity — and headloss is the variable
 * you select to find a bottleneck, where which way the pipe is running is
 * half the reading.
 *
 * Status gets presence instead. Its palette separates open from closed, but
 * not an open pipe conveying from an open pipe standing idle, which is what
 * a dead leg or an isolated zone looks like. Nothing else on the canvas
 * shows that without changing variable.
 *
 * Quality is carried rather than measured. A concentration, an age or a
 * trace fraction travels at the water's velocity, so the motion is worth
 * showing — on a transport model, which way a constituent is going is most
 * of what you came to see — but it says nothing about the number the colour
 * is showing, so it must not look like the patterns that do.
 *
 * The line: motion shares colour's pattern where the two rise and fall
 * together, and takes its own where they are free to disagree.
 */
export function pulseKindFor(linkVar: LinkVariable): PulseKind {
  switch (linkVar) {
    case "flow":
    case "velocity":
    case "headloss":
      return "magnitude";
    case "quality":
      return "transport";
    case "status":
      return "presence";
  }
}

/** Whether a variable animates at all. */
export function pulseApplies(linkVar: LinkVariable): boolean {
  return pulseKindFor(linkVar) !== "none";
}

/**
 * The variables the legend offers its animation toggle for.
 *
 * Derived, because this was the third place the same list was written out
 * by hand — after the layer that draws the pulse and the clock that
 * advances it — and it is the one that decides whether the control is
 * reachable at all. Extending the other two without this one animates
 * nothing a user can switch on.
 *
 * Plain strings: the legend is engine-neutral and compares against catalog
 * variable ids, which it treats as opaque.
 */
export const ANIMATED_LINK_VARIABLES: readonly string[] =
  LINK_VARIABLES.filter(pulseApplies);

/**
 * What to say when the animation toggle is offered but does not apply.
 *
 * Derived from the same list, because this sentence was the fourth copy of
 * it — after the layer, the clock and the legend's own gate — and the only
 * one a user ever reads. It went on naming flow and velocity long after the
 * other three had been extended.
 *
 * The labels arrive from the caller rather than being written here: which
 * words name a variable is the engine's business, and the legend that shows
 * this sentence is shared by all of them.
 */
export function animationAppliesHint(
  label: (linkVar: LinkVariable) => string,
): string {
  const names = LINK_VARIABLES.filter(pulseApplies).map(label);
  return `Animation applies to ${new Intl.ListFormat("en", {
    style: "long",
    type: "conjunction",
  }).format(names)}`;
}

/**
 * The pattern the motion draws, as the shader's uniform takes it.
 *
 * A continuous wave for a rate, hard marks for a yes/no, soft parcels for
 * something being carried. Without this they rendered identically — the
 * same swell, only at different speeds — so a constant rate kept out of the
 * data on purpose was put back by the pixels, and Status appeared to claim
 * every open link moves at the same speed.
 *
 * `none` never reaches the shader; it answers with the wave so the value is
 * always a pattern rather than a sentinel someone has to remember to check.
 */
export function pulsePattern(kind: PulseKind): number {
  switch (kind) {
    case "presence":
      return 1;
    case "transport":
      return 2;
    default:
      return 0;
  }
}

/**
 * Reject floating-point noise, relative to the run's own scale.
 *
 * A closed link reads exactly zero, but a barely-fed dead end reads as some
 * tiny residual, and on a large network that residual can be an artefact of
 * the solve rather than water. Relative because flow units differ by model,
 * so no absolute figure is portable. Small enough to be about arithmetic
 * only: a link with any real flow in it is above this, and deciding what
 * counts as *worth* showing is not a decision to bury here.
 */
function isMoving(flow: number | null, flowMax: number): boolean {
  if (flow == null || !Number.isFinite(flow)) return false;
  const eps = Math.max(1e-12, Math.abs(flowMax) * 1e-9);
  return Math.abs(flow) > eps;
}

/**
 * Signed pulse rate for one link: sign is direction, magnitude is rate.
 *
 * Zero means still. Direction comes from the sign of flow in both modes,
 * so a reversed link reverses on screen without the geometry being rebuilt.
 */
export function pulseSpeed(
  kind: PulseKind,
  link: { flow?: number | null; velocity?: number | null },
  flowMax: number,
): number {
  if (kind === "none") return 0;
  const flow = link.flow ?? null;
  const velocity = link.velocity ?? null;
  const dir = flow != null && flow < 0 ? -1 : 1;

  if (kind === "presence") {
    // Velocity alone is enough to say it is moving: a model can report one
    // without the other, and a link that is moving should not read as still
    // because of which column was written.
    const moving =
      isMoving(flow, flowMax) ||
      (velocity != null && Number.isFinite(velocity) && velocity !== 0);
    return moving ? PRESENCE_SPEED * dir : 0;
  }

  // `magnitude` and `transport` both take their rate from the water itself.
  // For transport that is not a stylistic choice: the parcels are the
  // engine's volume segments, and they travel at the water's speed because
  // that is what the solver moves them at.
  if (velocity != null && velocity > 0) {
    return Math.min(1, velocity / FULL_RATE_VELOCITY) * dir;
  }
  if (flow != null) {
    return Math.min(1, Math.abs(flow) / Math.max(0.01, flowMax)) * dir;
  }
  return UNKNOWN_SPEED * dir;
}
