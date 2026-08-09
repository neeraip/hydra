/**
 * The node pulse: a ring growing outward from a node, for variables that
 * are *rates* leaving or entering the network.
 *
 * The same rule the link pulse follows (see `linkPulse`): motion is worth
 * showing where it says something the colour cannot, and must not imply a
 * rate where there is only a state. A node's depth, head or stored volume
 * are states — a full manhole is not a fast one — so a ring pacing itself
 * against them would assert a speed the number does not have. Flooding and
 * the inflows are rates, and a ring expanding from the node is what one
 * looks like.
 *
 * Which ids qualify is the engine's answer, published in the component
 * registry and asked through `animatesVariable`; this module owns only how
 * a qualifying value becomes motion.
 *
 * Flooding earns it twice over. It is zero at nearly every node, so a
 * colour ramp over it is almost uniform and the few nodes that matter are
 * hard to pick out of a large network — while motion on exactly those
 * nodes is visible without hunting. That same sparsity is what keeps the
 * animation cheap: a still node draws no ring at all, so the moving set is
 * as small as the flooding is.
 */

import { isMoving } from "./linkPulse";

/**
 * How fast one node's ring expands: 0 (still) to 1 (full rate).
 *
 * Magnitude only. A node variable's sign says which way water crosses the
 * boundary, but a ring has one honest direction — outward, away from the
 * node — because that is the shape of water arriving at the surface, which
 * is what these variables measure. Encoding sign as an inward ring would
 * animate water being *un*-flooded, which nothing in the results means.
 */
export function ringRate(value: number | undefined, scale: number): number {
  // The same test the link pulse applies, deliberately shared rather than
  // restated: "still" must mean one thing on a canvas that pulses links
  // and rings nodes at once. Written separately here it drifted to a
  // thousand times stricter, which held real overflows — 5e-6 of a peak
  // measured in units — perfectly still while the number beside them said
  // otherwise. That threshold is about arithmetic noise only; what is
  // worth *showing* is not a decision to bury in it.
  if (!isMoving(value, scale)) return 0;
  const span = Math.max(Math.abs(scale), 1e-9);
  return Math.min(1, Math.abs(value as number) / span);
}

/** Whether a node shows a ring at all — the test that keeps the animated
 * set small, and the reason a quiet network costs nothing to animate. */
export function ringApplies(value: number | undefined, scale: number): boolean {
  return ringRate(value, scale) > 0;
}
