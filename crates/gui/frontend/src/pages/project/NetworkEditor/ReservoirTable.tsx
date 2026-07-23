import type React from "react";
import type { ReservoirRow } from "../../../hooks";
import {
  EditableCell,
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "./TablePrimitives";

const COL_COUNT = 5;

export function ReservoirTable({
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
}: {
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
}) {
  const tdStyle: React.CSSProperties = {
    padding: "7px 10px",
    fontSize: 12,
    fontFamily: "var(--font-mono)",
    borderBottom: "1px solid var(--border)",
  };
  const { virtualItems, paddingTop, paddingBottom } = useVirtualRows(
    rows,
    scrollContainerRef,
  );

  return (
    <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
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
            label="Head"
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
                display={isPendingRow ? "" : `${row.head} m`}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`reservoir:${row.id}:head`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("reservoir", row.id, "head", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-headPattern`}
                display={isPendingRow ? "" : (row.pattern ?? "—")}
                value={isPendingRow ? "" : (row.pattern ?? "")}
                placeholder={isPendingRow || row.pattern == null}
                isPending={pendingKeys.has(`reservoir:${row.id}:headPattern`)}
                onCommit={(v) => onPatch("reservoir", row.id, "headPattern", v)}
              />
              <EditableCell
                key={`${discardGen}-${row.id}-x`}
                display={isPendingRow ? "" : String(row.x)}
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
                display={isPendingRow ? "" : String(row.y)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`reservoir:${row.id}:y`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("reservoir", row.id, "y", parseFloat(v))
                }
              />
            </tr>
          );
        })}
        <VirtualSpacerRow height={paddingBottom} colSpan={COL_COUNT} />
      </tbody>
    </table>
  );
}
