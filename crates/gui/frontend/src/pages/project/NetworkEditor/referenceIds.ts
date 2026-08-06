/**
 * Which ids a reference field may name.
 *
 * Several editor columns hold not a value but a *reference*: a reservoir's
 * head pattern, a tank's volume curve, a pump's head curve, a valve's
 * curve. They were free text, so a typo — or a half-remembered name —
 * committed a dangling reference. The model validator catches it, but only
 * after the fact and only in the Issues panel; the cell that accepted it
 * said nothing.
 *
 * The set has to be draft-aware. Curves and patterns are created and
 * deleted in the same unsaved draft as the elements referencing them, so
 * "what exists" is not what the last save wrote: a curve added minutes ago
 * in the Curves tab is a perfectly good reference, and one staged for
 * deletion is not. Answering from the saved network alone would reject the
 * first and accept the second.
 */

/**
 * Ids that will exist once the current draft is saved: what the network
 * holds, plus staged additions, minus staged deletions.
 *
 * Sorted, so the suggestion list has a stable order rather than one that
 * depends on when each id happened to be created.
 */
export function referenceIds(
  saved: readonly string[],
  added: Iterable<string>,
  deleted: ReadonlySet<string>,
): string[] {
  const ids = new Set(saved);
  for (const id of added) ids.add(id);
  for (const id of deleted) ids.delete(id);
  return [...ids].sort((a, b) => a.localeCompare(b));
}

/**
 * Why `value` is not an acceptable reference, or `null` when it is.
 *
 * Empty is always acceptable: these references are optional, and clearing
 * one is how you say a reservoir has no head pattern.
 */
export function referenceError(
  value: string,
  allowed: readonly string[],
): string | null {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  return allowed.includes(trimmed) ? null : "No such id";
}
