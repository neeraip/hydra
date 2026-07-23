/**
 * Pure search / sort helpers for the Network Editor element tables.
 *
 * Kept free of React so they can be unit-tested in a plain Node environment,
 * and so the hot per-keystroke filter path stays a simple string scan.
 */

/**
 * Shared collator for element-ID comparisons. `String.prototype.localeCompare`
 * re-resolves locale data on every call, which is measurably slower when
 * sorting tens of thousands of ids; a single `Intl.Collator` instance keeps
 * identical ordering semantics at a fraction of the cost.
 */
export const idCollator = new Intl.Collator();

/** `Array.prototype.sort` comparator using the shared collator. */
export function compareIds(a: string, b: string): number {
  return idCollator.compare(a, b);
}

/** Debounce applied to the Elements search input before filtering runs. */
export const SEARCH_DEBOUNCE_MS = 150;

/**
 * Fields are joined with NUL so a query can never accidentally match across
 * two adjacent field values (NUL cannot be typed into the search input).
 */
const FIELD_SEPARATOR = "\u0000";

/**
 * Builds the lowercase search haystack for one row: every field value,
 * stringified and lowercased, NUL-joined. Matches the historical behaviour of
 * `Object.values(row).some((v) => String(v).toLowerCase().includes(q))`.
 */
export function buildRowHaystack(row: object): string {
  return Object.values(row)
    .map((v) => String(v).toLowerCase())
    .join(FIELD_SEPARATOR);
}

// Haystacks are cached per row object: row arrays are referentially stable
// across renders (see the useMemo wrappers in hooks/editors.ts), so repeated
// keystrokes reduce to one `.includes` per row instead of re-lowercasing
// every field of ~46k rows. Rows are never mutated in place — edits are
// staged in the draft store and produce fresh row objects on save.
const haystackCache = new WeakMap<object, string>();

/** Cached variant of {@link buildRowHaystack}. */
export function getRowHaystack(row: object): string {
  let haystack = haystackCache.get(row);
  if (haystack === undefined) {
    haystack = buildRowHaystack(row);
    haystackCache.set(row, haystack);
  }
  return haystack;
}

/**
 * Filter + sort one table's rows.
 *
 * - Empty `query` and `sortField === null` returns the input array untouched
 *   (no copy) so memoized consumers keep referential stability.
 * - String comparisons use the shared collator (same ordering as
 *   `localeCompare`, without the per-call locale lookup).
 */
export function filterSortRows<T extends object>(
  rows: T[],
  query: string,
  sortField: string | null,
  sortAsc: boolean,
): T[] {
  const q = query.toLowerCase();
  const filtered = q ? rows.filter((r) => getRowHaystack(r).includes(q)) : rows;
  if (!sortField) return filtered;
  const field = sortField;
  return [...filtered].sort((a, b) => {
    const av = (a as Record<string, unknown>)[field];
    const bv = (b as Record<string, unknown>)[field];
    if (typeof av === "number" && typeof bv === "number")
      return sortAsc ? av - bv : bv - av;
    return sortAsc
      ? idCollator.compare(String(av), String(bv))
      : idCollator.compare(String(bv), String(av));
  });
}

/**
 * Variant of {@link filterSortRows} that pins `pinnedRows` (pending, unsaved
 * rows) at the top of the result, ahead of the filtered + sorted existing
 * rows.
 *
 * Pinned rows are exempt from both the query filter and the active sort:
 * a freshly added row is mostly empty (it would match almost no query) and
 * has a placeholder temp id (it would sort to an arbitrary position — at
 * ~46k rows that means landing thousands of rows off-screen). Keeping new
 * rows grouped at the top makes them immediately visible regardless of the
 * user's current search/sort state.
 *
 * With no pinned rows this returns exactly what {@link filterSortRows}
 * returns (including the untouched input array for empty query + no sort),
 * so memoized consumers keep referential stability.
 */
export function filterSortRowsWithPinned<T extends object>(
  existingRows: T[],
  pinnedRows: T[],
  query: string,
  sortField: string | null,
  sortAsc: boolean,
): T[] {
  const body = filterSortRows(existingRows, query, sortField, sortAsc);
  if (pinnedRows.length === 0) return body;
  return [...pinnedRows, ...body];
}

/**
 * Above this option count the reference-input `<datalist>` is dropped
 * entirely. A single shared datalist is fine at moderate sizes (it is ~N DOM
 * nodes rendered once per table), but at tens of thousands of options the
 * browser's built-in typing filter itself becomes the bottleneck — every
 * keystroke re-scans the full option list on the UI thread. RefInputCell
 * already validates the typed id against the option list on blur (invalid id
 * ⇒ error style), so dropping the datalist at large N only loses
 * autocomplete convenience, never correctness.
 */
export const REF_DATALIST_MAX_OPTIONS = 5000;

/** Whether a reference-input datalist should be rendered for N options. */
export function shouldUseRefDatalist(optionCount: number): boolean {
  return optionCount <= REF_DATALIST_MAX_OPTIONS;
}
