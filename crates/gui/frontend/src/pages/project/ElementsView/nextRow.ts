/**
 * The row the contents panel's add button appends (hydra-common
 * §4.5.2.2).
 *
 * It used to be a row of zeros, and for most tables a row of zeros is
 * one the write can only refuse: a curve's abscissae must ascend, a
 * series' times must increase, and both passed zero at their first row.
 * The add button offered a row that could never land.
 *
 * So the seed starts from the last row — its values are the best guess
 * available for the new row's — and moves the advancing column past the
 * end, by the table's own last step so an hourly series stays hourly.
 * Which column must advance is the engine's knowledge, not something
 * headings reveal, which is why the detail serves it (`advances`).
 */
export function nextRow(
  rows: number[][],
  columns: number,
  advances: number | undefined,
): number[] {
  const last = rows[rows.length - 1];
  if (!last) return Array.from({ length: columns }, () => 0);
  const prev = rows[rows.length - 2];
  return last.map((value, i) => {
    if (i !== advances) return value;
    const step = prev ? value - prev[i] : 0;
    return value + (step > 0 ? step : 1);
  });
}
