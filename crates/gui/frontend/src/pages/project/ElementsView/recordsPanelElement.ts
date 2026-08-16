/**
 * Which element the Editor's records panel shows, if any.
 *
 * A spatial kind follows the canvas selection; a collection follows the
 * Editor's own opened container, because a curve or a control measure
 * has no geometry and therefore no canvas selection to follow.
 *
 * This decision used to be `spatial && selectedId` inline in the view,
 * which answered "never" for every container — so a control measure's
 * six layers, a snow pack's surfaces and a unit hydrograph's monthly
 * responses were served by the backend, had a panel able to draw them,
 * and appeared nowhere in the running app. The backend tests passed and
 * the panel's tests passed; the gate between them was the only thing
 * wrong, and nothing tested the gate.
 */
export function recordsPanelElement(
  spatial: boolean,
  selectedId: string | null | undefined,
  openContainer: string | null | undefined,
): string | null {
  if (spatial) return selectedId ?? null;
  return openContainer ?? null;
}
