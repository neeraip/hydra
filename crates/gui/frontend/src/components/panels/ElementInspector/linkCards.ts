/**
 * Which result values a link's inspector card shows.
 *
 * This was a run of hand-written `if`s, one per variable, and it had
 * drifted from the variable list in two directions at once. `headloss`
 * was missing altogether — selectable on the canvas, coloured by the
 * legend, and never rendered here, so choosing it showed Flow instead.
 * Meanwhile `status` and `quality` were pushed unconditionally, so a run
 * with no water-quality results still displayed a Quality box reading
 * "—", advertising a variable the selector did not even offer.
 *
 * Both are the same defect: the set of cards was written out by hand
 * instead of derived from the variables a link actually has. So it is
 * derived here, and the test walks the variable union — which is what
 * makes a newly added variable fail loudly rather than silently vanish.
 */

import type { LinkVariable } from "../../../canvas/types";
import type { Quantity } from "../../../units";
import { headlossQuantity } from "./seriesCache";

/**
 * Every link variable, in the order their cards read.
 *
 * Typed as the full union rather than an array of strings so that adding
 * a `LinkVariable` and forgetting this list is a compile error.
 */
export const LINK_CARD_ORDER: readonly LinkVariable[] = [
  "flow",
  "velocity",
  "status",
  "headloss",
  "quality",
];

/**
 * Decimal places for each measured variable in SI display.
 *
 * SI only: the US units are coarser, so their own defaults already read
 * well and are left to `formatQty`. Head loss gets three because m/km
 * runs small on short pipes, where two would show every value as 0.00.
 */
export const LINK_SI_DECIMALS: Partial<Record<LinkVariable, number>> = {
  flow: 2,
  velocity: 3,
  headloss: 3,
};

/** The subset of a link's fields these cards read. */
export type LinkResultValues = {
  [K in LinkVariable]?: number | null;
};

/**
 * The variable shown large, and the ones shown beneath it.
 *
 * A variable appears only when the link carries a value for it, which is
 * what stops an engine or a run without quality results from advertising
 * a Quality box. The selected variable leads when it is one of them;
 * otherwise the first available does, so the card is never empty while
 * the link has something to say.
 *
 * @param link    the link's result values.
 * @param linkVar the variable currently selected on the canvas.
 */
export function linkCardVariables(
  link: LinkResultValues,
  linkVar?: LinkVariable,
): { primary: LinkVariable; secondaries: LinkVariable[] } {
  const present = LINK_CARD_ORDER.filter((v) => link[v] != null);
  const primary =
    linkVar && present.includes(linkVar) ? linkVar : (present[0] ?? "flow");
  return { primary, secondaries: present.filter((v) => v !== primary) };
}

/**
 * What to call a link's head loss.
 *
 * A pipe reports head loss per unit length and everything else reports it
 * outright, so the two are different quantities carrying different units —
 * `headlossQuantity` is the authority, and this follows it rather than
 * deciding again. Labelling both "Headloss" would put m/km and m under one
 * word.
 */
export function headlossLabel(linkType: string | undefined): string {
  return headlossQuantity(linkType) === "headloss"
    ? "Unit Headloss"
    : "Headloss";
}

/** Display label for a link variable's card. */
export function linkCardLabel(
  variable: LinkVariable,
  linkType: string | undefined,
): string {
  switch (variable) {
    case "flow":
      return "Flow";
    case "velocity":
      return "Velocity";
    case "status":
      return "Status";
    case "headloss":
      return headlossLabel(linkType);
    case "quality":
      return "Quality";
  }
}

/**
 * The §5 quantity a link variable's value is expressed in, or `undefined`
 * where the value is not a physical measure — status is a code and
 * quality is a bare concentration.
 */
export function linkCardQuantity(
  variable: LinkVariable,
  linkType: string | undefined,
): Quantity | undefined {
  switch (variable) {
    case "flow":
      return "flow";
    case "velocity":
      return "velocity";
    case "headloss":
      return headlossQuantity(linkType);
    default:
      return undefined;
  }
}
