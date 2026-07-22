import type React from "react";
import { useEffect, useRef } from "react";
import type { PumpRow } from "../../../hooks";
import {
  EditableCell,
  RefInputCell,
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "./TablePrimitives";

const COL_COUNT = 6;

export function PumpTable({
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
  scrollContainerRef,
  focusToken,
}: {
  rows: PumpRow[];
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
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  /** Bumped (e.g. `Date.now()`) whenever `selectedId` should be scrolled into
   *  view, such as a jump from the Pump curves tab's "attached to" link. */
  focusToken?: number;
}) {
  const tdStyle: React.CSSProperties = {
    padding: "7px 10px",
    fontSize: 12,
    fontFamily: "var(--font-mono)",
    borderBottom: "1px solid var(--border)",
  };
  const { virtualItems, paddingTop, paddingBottom, virtualizer } =
    useVirtualRows(rows, scrollContainerRef);
  const appliedFocusToken = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (focusToken == null || focusToken === appliedFocusToken.current) return;
    if (!selectedId) return;
    const idx = rows.findIndex((r) => r.id === selectedId);
    if (idx >= 0) {
      virtualizer.scrollToIndex(idx, { align: "center" });
      appliedFocusToken.current = focusToken;
    }
  }, [focusToken, selectedId, rows, virtualizer]);

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
            field="curve"
            label="Curve"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
          />
          <SortTh
            field="powerKw"
            label="Power"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="speed"
            label="Speed"
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
                  isPending={pendingKeys.has(`pump:${row.id}:id`)}
                  onCommit={(v) => onPatch("pump", row.id, "id", v.trim())}
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
                  listId={`pump-from-${row.id}`}
                  isPending={pendingKeys.has(`pump:${row.id}:from`)}
                  onCommit={(v) => onPatch("pump", row.id, "from", v)}
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
                  listId={`pump-to-${row.id}`}
                  isPending={pendingKeys.has(`pump:${row.id}:to`)}
                  onCommit={(v) => onPatch("pump", row.id, "to", v)}
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
                key={`${discardGen}-${row.id}-curve`}
                display={isPendingRow ? "" : (row.curve ?? "—")}
                value={isPendingRow ? "" : (row.curve ?? "")}
                placeholder={isPendingRow || row.curve == null}
                isPending={pendingKeys.has(`pump:${row.id}:curve`)}
                onCommit={(v) => onPatch("pump", row.id, "curve", v)}
              />
              <EditableCell
                key={`${discardGen}-${row.id}-powerKw`}
                display={
                  isPendingRow
                    ? ""
                    : row.powerKw != null
                      ? `${row.powerKw.toFixed(1)} kW`
                      : "—"
                }
                value={
                  isPendingRow
                    ? ""
                    : row.powerKw != null
                      ? String(row.powerKw.toFixed(1))
                      : ""
                }
                placeholder={isPendingRow || row.powerKw == null}
                align="right"
                isPending={pendingKeys.has(`pump:${row.id}:powerKw`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch("pump", row.id, "powerKw", parseFloat(v))
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-speed`}
                display={isPendingRow ? "" : row.speed.toFixed(2)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`pump:${row.id}:speed`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch("pump", row.id, "speed", parseFloat(v))
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
