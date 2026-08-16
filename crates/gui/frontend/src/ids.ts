// ── Suggesting a free element id ─────────────────────────────────────────────
//
// One loop, three callers: the canvas suggesting a node id, the canvas
// suggesting a link id, the Editor's Add dialog suggesting one for the
// kind its table shows. Each had its own copy, and the copies had begun
// to differ in the one place a copy differs first — what happens when the
// loop runs out. Two returned `prefix + Date.now()`, one returned the
// pool's size plus one, and all three capped the scan at 9999 for no
// reason a comment could state.
//
// What the callers still own is the question this function refuses to
// answer: *which ids are taken*. The two engines disagree about it — a
// drainage model has one namespace for everything, water distribution
// keeps nodes and links apart — and no contract section says which pool a
// new id must be free in, so each surface passes the pool it can see.

/**
 * The first `prefix``n` (n = 1, 2, 3…) not present in `taken`.
 *
 * Always terminates without a cap or a fallback: `taken` is finite, so
 * among the first `taken.size + 1` candidates at least one is free. The
 * suggestion is only a suggestion either way — the create refuses a
 * genuine collision by name.
 */
export function firstFreeId(
  prefix: string,
  taken: ReadonlySet<string>,
): string {
  for (let i = 1; i <= taken.size + 1; i += 1) {
    const candidate = `${prefix}${i}`;
    if (!taken.has(candidate)) return candidate;
  }
  // Unreachable: the loop admits more candidates than `taken` has entries.
  return `${prefix}${taken.size + 1}`;
}
