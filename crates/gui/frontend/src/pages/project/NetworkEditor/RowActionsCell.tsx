import {
  MapPinIcon,
  PencilSquareIcon,
  TrashIcon,
} from "@heroicons/react/16/solid";
import {
  ActionIcon,
  RowActionsCell as ActionsCell,
} from "../../../components/panels/editorTable";
import { ELEMENT_TEMP_ID_PREFIX } from "../../../hooks/DraftContext";

// ─────────────────────────────────────────────────────────────────────────────
// Per-row action icons for the element tables: Show on map · Rename · Delete.
//
// These are selection-scoped actions that used to crowd the editor toolbar and
// steal width from the kind tabs. They now live on the row, revealed on hover
// (CSS `.ne-row-actions`) and kept visible on the selected row. Enablement is
// derived per row: temp (unsaved) rows aren't on the map and can't be renamed;
// a row with staged edits can't be renamed (the rename is immediate and would
// desync the draft, which keys on the id).
// ─────────────────────────────────────────────────────────────────────────────

export type RowAction = "map" | "rename" | "delete";

export { ActionsTh } from "../../../components/panels/editorTable";

export function RowActionsCell({
  kind,
  id,
  isSelected,
  pendingKeys,
  pendingRowIds,
  onAction,
}: {
  kind: string;
  id: string;
  isSelected: boolean;
  /** Full draft keys (`kind:id:field`) — used to detect staged edits on this row. */
  pendingKeys: Set<string>;
  /** Temp ids of this kind's unsaved rows. */
  pendingRowIds?: Set<string>;
  onAction: (action: RowAction, kind: string, id: string) => void;
}) {
  const isTemp =
    (pendingRowIds?.has(id) ?? false) || id.startsWith(ELEMENT_TEMP_ID_PREFIX);
  let hasStagedEdits = false;
  if (!isTemp) {
    const prefix = `${kind}:${id}:`;
    for (const k of pendingKeys) {
      if (k.startsWith(prefix)) {
        hasStagedEdits = true;
        break;
      }
    }
  }

  return (
    <ActionsCell selected={isSelected}>
      <ActionIcon
        title="Show on map"
        disabledReason={isTemp ? "Save the row first to locate it" : undefined}
        onClick={() => onAction("map", kind, id)}
      >
        <MapPinIcon style={{ width: 13, height: 13 }} />
      </ActionIcon>
      <ActionIcon
        title="Rename"
        disabledReason={
          isTemp
            ? "Save the row first to rename it"
            : hasStagedEdits
              ? "Save or discard this row's edits to rename"
              : undefined
        }
        onClick={() => onAction("rename", kind, id)}
      >
        <PencilSquareIcon style={{ width: 13, height: 13 }} />
      </ActionIcon>
      <ActionIcon
        title="Delete"
        danger
        onClick={() => onAction("delete", kind, id)}
      >
        <TrashIcon style={{ width: 13, height: 13 }} />
      </ActionIcon>
    </ActionsCell>
  );
}
