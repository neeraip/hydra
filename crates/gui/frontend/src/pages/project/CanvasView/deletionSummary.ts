import type { Removed } from "../../../hooks";

/**
 * What to tell the user a delete actually did.
 *
 * A delete is rarely one element. Removing a node takes the links
 * attached to it, and in a drainage model it also takes the records that
 * only described it — an inflow, a treatment, a sewer-inflow
 * assignment. Those are correct removals, and they are also the ones a
 * user does not expect, so the message names them rather than leaving
 * them to be discovered when a run comes back different.
 *
 * `null` when nothing but the element itself went: the canvas already
 * shows it gone, and a toast repeating what the screen just did is
 * noise.
 */
export function deletionSummary(removed: Removed): string | null {
  const parts: string[] = [];
  if (removed.links.length > 0) {
    parts.push(
      removed.links.length <= 3
        ? removed.links.join(", ")
        : `${removed.links.length} links`,
    );
  }
  parts.push(...removed.attachments);
  if (parts.length === 0) return null;
  return `Deleted ${removed.id} and ${sentenceList(parts)}.`;
}

/** "a", "a and b", "a, b and c" — the last join is "and", not a comma. */
function sentenceList(items: string[]): string {
  if (items.length === 1) return items[0];
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}
