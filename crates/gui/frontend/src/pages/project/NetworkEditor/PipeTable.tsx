import type React from "react";
import type { PipeRow } from "../../../hooks";
import { EditableCell, RefInputCell, SortTh } from "./TablePrimitives";

export function PipeTable({
  rows,
  sortField,
  sortAsc,
  selectedId,
  onSort,
  onSelect,
  onPatch,
  nodeOptions,
  pendingKeys,
  pendingRowIds,
  discardGen,
}: {
  rows: PipeRow[];
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
  nodeOptions: string[];
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
            field="from"
            label="From"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
          />
          <SortTh
            field="to"
            label="To"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
          />
          <SortTh
            field="length"
            label="Length"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="diameter"
            label="Ø"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="roughness"
            label="Roughness"
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
                  isPending={pendingKeys.has(`pipe:${row.id}:id`)}
                  onCommit={(v) => onPatch("pipe", row.id, "id", v.trim())}
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
              {isPendingRow ? (
                <RefInputCell
                  value={row.from}
                  placeholder="Select node"
                  options={nodeOptions}
                  listId={`pipe-from-${row.id}`}
                  isPending={pendingKeys.has(`pipe:${row.id}:from`)}
                  onCommit={(v) => onPatch("pipe", row.id, "from", v)}
                />
              ) : (
                <td
                  style={{
                    ...tdStyle,
                    fontFamily: "var(--font-ui)",
                    color: "var(--text-secondary)",
                  }}
                >
                  {row.from}
                </td>
              )}
              {isPendingRow ? (
                <RefInputCell
                  value={row.to}
                  placeholder="Select node"
                  options={nodeOptions}
                  listId={`pipe-to-${row.id}`}
                  isPending={pendingKeys.has(`pipe:${row.id}:to`)}
                  onCommit={(v) => onPatch("pipe", row.id, "to", v)}
                />
              ) : (
                <td
                  style={{
                    ...tdStyle,
                    fontFamily: "var(--font-ui)",
                    color: "var(--text-secondary)",
                  }}
                >
                  {row.to}
                </td>
              )}
              <EditableCell
                key={`${discardGen}-${row.id}-length`}
                display={isPendingRow ? "" : `${row.length} m`}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`pipe:${row.id}:length`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch("pipe", row.id, "length", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-diameter`}
                display={isPendingRow ? "" : `${row.diameter} mm`}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`pipe:${row.id}:diameter`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch("pipe", row.id, "diameter", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-roughness`}
                display={isPendingRow ? "" : String(row.roughness)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-tertiary)" }}
                isPending={pendingKeys.has(`pipe:${row.id}:roughness`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch("pipe", row.id, "roughness", parseFloat(v))
                }
              />
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
