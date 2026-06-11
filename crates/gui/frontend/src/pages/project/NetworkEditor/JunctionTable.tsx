import type React from "react";
import type { JunctionRow } from "../../../hooks";
import { EditableCell, SortTh } from "./TablePrimitives";

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
}) {
  const tdStyle: React.CSSProperties = {
    padding: "7px 10px",
    fontSize: 12,
    fontFamily: "var(--font-mono)",
    borderBottom: "1px solid var(--border)",
  };

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
            field="elevation"
            label="Elevation"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="demand"
            label="Demand"
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
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => {
          const isSelected = selectedId === row.id;
          const isPendingRow = pendingRowIds?.has(row.id) ?? false;
          return (
            <tr
              key={row.id}
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
                display={isPendingRow ? "" : `${row.elevation} m`}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`junction:${row.id}:elevation`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("junction", row.id, "elevation", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-baseDemand`}
                display={isPendingRow ? "" : `${row.baseDemand.toFixed(2)} L/s`}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`junction:${row.id}:baseDemand`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch("junction", row.id, "baseDemand", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-x`}
                display={isPendingRow ? "" : String(row.x)}
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
                display={isPendingRow ? "" : String(row.y)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`junction:${row.id}:y`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch("junction", row.id, "y", parseFloat(v))
                }
              />
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
