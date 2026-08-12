import type React from "react";
import {
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "../../../components/panels/editorTable";
import type { ReservoirRow } from "../../../hooks";
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

export function ReservoirTable({
  referenceIds,
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
  /** Ids this table's reference column may name (draft-aware). */
  referenceIds: readonly string[];
  rows: ReservoirRow[];
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
            field="head"
            label={`Head (${unitLabel("head", sys)})`}
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="pattern"
            label="Head pattern"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
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
                  isPending={pendingKeys.has(`reservoir:${row.id}:id`)}
                  onCommit={(v) => onPatch("reservoir", row.id, "id", v.trim())}
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
                key={`${discardGen}-${row.id}-head`}
                display={
                  isPendingRow ? "" : formatQtyValue(row.head, "head", sys)
                }
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`reservoir:${row.id}:head`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch(
                    "reservoir",
                    row.id,
                    "head",
                    fromDisplay(parseFloat(v), "head", sys),
                  )
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-headPattern`}
                display={isPendingRow ? "" : (row.pattern ?? "—")}
                value={isPendingRow ? "" : (row.pattern ?? "")}
                placeholder={isPendingRow || row.pattern == null}
                isPending={pendingKeys.has(`reservoir:${row.id}:headPattern`)}
                onCommit={(v) => onPatch("reservoir", row.id, "headPattern", v)}
                options={referenceIds}
              />
              <EditableCell
                key={`${discardGen}-${row.id}-x`}
                display={isPendingRow ? "" : formatCoordValue(row.x)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`reservoir:${row.id}:x`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("reservoir", row.id, "x", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-y`}
                display={isPendingRow ? "" : formatCoordValue(row.y)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`reservoir:${row.id}:y`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("reservoir", row.id, "y", parseFloat(v))
                }
              />
              <RowActionsCell
                kind="reservoir"
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
