import {
  MapPinIcon,
  PencilSquareIcon,
  TrashIcon,
} from "@heroicons/react/16/solid";
import type React from "react";
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

/** Trailing header cell for the actions column (blank, narrow). */
export function ActionsTh() {
  return (
    <th
      aria-label="Actions"
      style={{
        width: 1,
        borderBottom: "1px solid var(--border)",
        padding: "7px 10px",
      }}
    />
  );
}

const iconBtnBase: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 22,
  height: 22,
  padding: 0,
  border: "none",
  borderRadius: 4,
  background: "transparent",
  cursor: "pointer",
};

function ActionIcon({
  title,
  disabled,
  danger,
  onClick,
  children,
}: {
  title: string;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      data-tooltip={title}
      onClick={(e) => {
        e.stopPropagation();
        if (!disabled) onClick();
      }}
      style={{
        ...iconBtnBase,
        color: disabled
          ? "var(--text-disabled)"
          : danger
            ? "rgba(230, 120, 120, 0.9)"
            : "var(--text-secondary)",
        cursor: disabled ? "not-allowed" : "pointer",
      }}
      onMouseEnter={(e) => {
        if (disabled) return;
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--bg-card-hover)";
        (e.currentTarget as HTMLButtonElement).style.color = danger
          ? "rgb(240, 130, 130)"
          : "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = "transparent";
        (e.currentTarget as HTMLButtonElement).style.color = disabled
          ? "var(--text-disabled)"
          : danger
            ? "rgba(230, 120, 120, 0.9)"
            : "var(--text-secondary)";
      }}
    >
      {children}
    </button>
  );
}

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
    <td
      style={{
        borderBottom: "1px solid var(--border)",
        padding: "0 8px",
        textAlign: "right",
        whiteSpace: "nowrap",
        width: 1,
      }}
    >
      <div
        className={`ne-row-actions${isSelected ? " is-visible" : ""}`}
        style={{ display: "inline-flex", gap: 1 }}
      >
        <ActionIcon
          title={isTemp ? "Save the row first to locate it" : "Show on map"}
          disabled={isTemp}
          onClick={() => onAction("map", kind, id)}
        >
          <MapPinIcon style={{ width: 13, height: 13 }} />
        </ActionIcon>
        <ActionIcon
          title={
            isTemp
              ? "Save the row first to rename it"
              : hasStagedEdits
                ? "Save or discard this row's edits to rename"
                : "Rename"
          }
          disabled={isTemp || hasStagedEdits}
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
      </div>
    </td>
  );
}
