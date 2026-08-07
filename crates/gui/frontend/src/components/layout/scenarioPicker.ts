/**
 * What the jump-to-scenario picker offers.
 *
 * The base model is not a scenario. It has no row in the scenarios table
 * and no id — it is what `activeScenarioId === null` means — so anything
 * built by listing scenario records leaves it out. The strip beside this
 * picker draws it as a separate chip for exactly that reason, and the
 * picker, which listed records, could not reach it at all: the one thing
 * every project has was the one thing you could not jump to.
 *
 * So the option list is built here rather than being a filtered array of
 * records, and the base model is a member of it with a `null` id — the same
 * `null` the rest of the app already means by "the base model".
 */

export interface ScenarioPickerOption {
  /** `null` is the base model, which has no record of its own. */
  id: string | null;
  name: string;
  /**
   * The record's state, left as a string.
   *
   * The toolbar keeps a union of the states it draws, and that union does
   * not include `stale` — which the rows nonetheless test for, because a
   * record can carry it. Narrowing here would either drop that state or
   * commit this module to a list it cannot see the whole of, and neither
   * reader needs it narrowed: one picks a colour, the other compares to a
   * literal.
   */
  state: string;
}

/**
 * What the base model is called wherever it is listed beside scenarios.
 *
 * A name, because a picker row without one is a blank line — and the same
 * name the strip's chip and its tooltip use, so the thing you click in one
 * place is the thing you recognise in the other.
 */
export const BASE_MODEL_NAME = "Base model";

/**
 * The rows to show, in tree order with the base model first.
 *
 * First because it is the root every scenario descends from, so a list that
 * put it anywhere else would be claiming a lineage it does not have.
 *
 * Filtering matches on name, and the base model is filtered like any other
 * row: someone typing "base" is looking for it, and someone typing the name
 * of a scenario is not.
 */
export function scenarioPickerOptions(
  scenarios: readonly { id: string; name: string; state: string }[],
  baseState: string,
  query: string,
): ScenarioPickerOption[] {
  const all: ScenarioPickerOption[] = [
    { id: null, name: BASE_MODEL_NAME, state: baseState },
    ...scenarios.map((s) => ({ id: s.id, name: s.name, state: s.state })),
  ];
  const q = query.trim().toLowerCase();
  return q ? all.filter((o) => o.name.toLowerCase().includes(q)) : all;
}
