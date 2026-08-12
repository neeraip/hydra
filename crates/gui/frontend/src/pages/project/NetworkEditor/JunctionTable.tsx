import type React from "react";
import {
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "../../../components/panels/editorTable";
import type { JunctionRow } from "../../../hooks";
import {
  formatCoordValue,
  formatQtyValue,
  fromDisplay,
  unitLabel,
  useUnitSystem,
} from "../../../units";
import { ActionsTh, type RowAction, RowActionsCell } from "./RowActionsCell";
import { EditableCell } from "./TablePrimitives";

const COL_COUNT = 6;

export function JunctionTable({
  rows,
  sortField,
  sortAsc,
  selectedId,
  onSort,
  onSelect,
  onPatch,
  pendingKeys,
  pendingRowIds,
  discardGen,
  scrollContainerRef,
  onRowAction,
}: {
  rows: JunctionRow[];
  sortField: string;
  sortAsc: boolean;
  selectedId: string | null;
  onSort: (f: string) => void;
  onSelect: (id: string) => void;
  onPatch: (
    kind: string,
    id: string,
    field: string,
    value: number | string,
  ) => void;
  pendingKeys: Set<string>;
  pendingRowIds?: Set<string>;
  discardGen: number;
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  onRowAction: (action: RowAction, kind: string, id: string) => void;
}) {
  const sys = useUnitSystem();
  const tdStyle: React.CSSProperties = {
    padding: "7px 10px",
    fontSize: "var(--text-md)",
    fontFamily: "var(--font-mono)",
    borderBottom: "1px solid var(--border)",
  };
  const { virtualItems, paddingTop, paddingBottom } = useVirtualRows(
    rows,
    scrollContainerRef,
  );

  return (
    <table
      style={{
        width: "100%",
        borderCollapse: "collapse",
        fontSize: "var(--text-lg)",
      }}
    >
      <thead>
        <tr>
          <SortTh
            field="id"
            label="ID"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
          />
          <SortTh
            field="elevation"
            label={`Elevation (${unitLabel("elevation", sys)})`}
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="baseDemand"
            label={`Demand (${unitLabel("demand", sys)})`}
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="x"
            label="X"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="y"
            label="Y"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <ActionsTh />
        </tr>
      </thead>
      <tbody>
        <VirtualSpacerRow height={paddingTop} colSpan={COL_COUNT} />
        {virtualItems.map((vi) => {
          const row = rows[vi.index];
          const isSelected = selectedId === row.id;
          const isPendingRow = pendingRowIds?.has(row.id) ?? false;
          return (
            <tr
              key={row.id}
              data-row-id={row.id}
              onClick={() => onSelect(row.id)}
              style={{
                cursor: "pointer",
                background: isSelected
                  ? "var(--accent-dim)"
                  : isPendingRow
                    ? "rgba(220, 160, 40, 0.05)"
                    : undefined,
                borderLeft: isSelected
                  ? "2px solid var(--accent)"
                  : "2px solid transparent",
              }}
              onMouseEnter={(e) => {
                if (!isSelected)
                  (e.currentTarget as HTMLTableRowElement).style.background =
                    "var(--bg-card-hover)";
              }}
              onMouseLeave={(e) => {
                if (!isSelected)
                  (e.currentTarget as HTMLTableRowElement).style.background =
                    "";
              }}
            >
              {isPendingRow ? (
                <EditableCell
                  key={`${discardGen}-${row.id}-id`}
                  display=""
                  placeholder
                  style={{ fontWeight: 500 }}
                  isPending={pendingKeys.has(`junction:${row.id}:id`)}
                  onCommit={(v) => onPatch("junction", row.id, "id", v.trim())}
                />
              ) : (
                <td
                  style={{
                    ...tdStyle,
                    fontWeight: 500,
                    color: "var(--text-primary)",
                  }}
                >
                  {row.id}
                </td>
              )}
              <EditableCell
                key={`${discardGen}-${row.id}-elevation`}
                display={
                  isPendingRow
                    ? ""
                    : formatQtyValue(row.elevation, "elevation", sys)
                }
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`junction:${row.id}:elevation`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch(
                    "junction",
                    row.id,
                    "elevation",
                    fromDisplay(parseFloat(v), "elevation", sys),
                  )
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-baseDemand`}
                display={
                  isPendingRow
                    ? ""
                    : formatQtyValue(
                        row.baseDemand,
                        "demand",
                        sys,
                        sys === "si" ? 2 : undefined,
                      )
                }
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`junction:${row.id}:baseDemand`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch(
                    "junction",
                    row.id,
                    "baseDemand",
                    fromDisplay(parseFloat(v), "demand", sys),
                  )
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-x`}
                display={isPendingRow ? "" : formatCoordValue(row.x)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`junction:${row.id}:x`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("junction", row.id, "x", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-y`}
                display={isPendingRow ? "" : formatCoordValue(row.y)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`junction:${row.id}:y`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("junction", row.id, "y", parseFloat(v))
                }
              />
              <RowActionsCell
                kind="junction"
                id={row.id}
                isSelected={isSelected}
                pendingKeys={pendingKeys}
                pendingRowIds={pendingRowIds}
                onAction={onRowAction}
              />
            </tr>
          );
        })}
        <VirtualSpacerRow height={paddingBottom} colSpan={COL_COUNT} />
      </tbody>
    </table>
  );
}
