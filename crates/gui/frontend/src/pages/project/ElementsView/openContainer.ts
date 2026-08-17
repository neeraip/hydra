/**
 * The Editor's container selection: which collection element is open,
 * and — the half that was missing — on which kind's tab it was opened.
 *
 * It was a bare id, and a bare id survives a tab switch it should not:
 * open a control measure on the LID tab, visit Curves, and every
 * container tab from Curves to Inlet designs went on showing the
 * measure's layer tables, because the id still answered and the backend
 * serves an element's records whatever kind the tab claims. The kind is
 * half the selection's identity, so it travels with the id and the
 * selection only answers on the tab it was made on.
 */
export interface OpenContainer {
  kind: string;
  id: string;
}

/** The open container's id, on `kind`'s tab and no other. */
export function openContainerOn(
  open: OpenContainer | null,
  kind: string | null,
): string | null {
  if (!open || !kind || open.kind !== kind) return null;
  return open.id;
}
