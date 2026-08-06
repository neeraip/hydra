/**
 * Which element is selected — as a class *and* an id, never an id alone.
 *
 * An element id is unique only within its class. EPANET keeps node and
 * link namespaces separate and the parser accepts that, so a junction `2`
 * and a pipe `2` are two different elements that happen to share a name;
 * the same is true of every engine the GUI serves. A bare id is therefore
 * not an identity, and code that treats it as one answers questions about
 * the wrong element.
 *
 * The network list did exactly that: it collapsed the three class-scoped
 * selections into `nodeId ?? linkId ?? regionId` and compared rows against
 * the result. Selecting junction `2` highlighted pipe `2` as well, and the
 * scroll-into-view jumped to whichever shared that name first — which
 * could be the other class entirely.
 */

import type { ElementClass } from "../../hooks";

/** The selected element, or `null` when nothing is selected. */
export interface ActiveElement {
  cls: ElementClass;
  id: string;
}

/**
 * The one selected element, from the three per-class selections.
 *
 * They are mutually exclusive in practice — selecting a link clears the
 * node — and the order here decides only what happens if that ever stops
 * being true.
 */
export function activeElement(
  nodeId?: string | null,
  linkId?: string | null,
  regionId?: string | null,
): ActiveElement | null {
  if (nodeId != null) return { cls: "point", id: nodeId };
  if (linkId != null) return { cls: "polyline", id: linkId };
  if (regionId != null) return { cls: "region", id: regionId };
  return null;
}

/** Whether a row *is* the selected element — same class and same id. */
export function isActiveRow(
  row: { cls: ElementClass; id: string },
  active: ActiveElement | null,
): boolean {
  return active != null && row.cls === active.cls && row.id === active.id;
}

/**
 * A key that changes exactly when the selected element does.
 *
 * Used to decide whether a new selection still needs scrolling to. Keying
 * on the id alone meant re-selecting the same name in another class read
 * as "already scrolled there" and the list stayed put. The separator is a
 * control character no id can contain, so no pair of (class, id) can
 * collide with another.
 */
export function activeKey(active: ActiveElement | null): string | null {
  return active && `${active.cls}${SEP}${active.id}`;
}

/** ASCII unit separator — cannot appear in an element id. */
const SEP = "\u001f";
