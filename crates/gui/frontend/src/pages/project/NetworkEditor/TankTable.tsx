import type React from "react";
import type { TankRow } from "../../../hooks";
import { formatQtyRaw, fromDisplay, useUnitSystem } from "../../../units";
import {
  EditableCell,
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "./TablePrimitives";

const COL_COUNT = 9;

export function TankTable({
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
  rows: TankRow[];
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
  const sys = useUnitSystem();
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
            field="elevation"
            label="Elevation"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="minLevel"
            label="Min level"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="initialLevel"
            label="Init level"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="maxLevel"
            label="Max level"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="diameter"
            label="Diameter"
            sortField={sortField}
            sortAsc={sortAsc}
            onSort={onSort}
            align="right"
          />
          <SortTh
            field="volumeCurve"
            label="Vol. curve"
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
                  isPending={pendingKeys.has(`tank:${row.id}:id`)}
                  onCommit={(v) => onPatch("tank", row.id, "id", v.trim())}
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
                    : formatQtyRaw(row.elevation, "elevation", sys)
                }
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`tank:${row.id}:elevation`)}
                inputType="number"
                onCommit={(v) =>
                  onPatch(
                    "tank",
                    row.id,
                    "elevation",
                    fromDisplay(parseFloat(v), "elevation", sys),
                  )
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-minLevel`}
                display={
                  isPendingRow ? "" : formatQtyRaw(row.minLevel, "length", sys)
                }
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-secondary)" }}
                isPending={pendingKeys.has(`tank:${row.id}:minLevel`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch(
                    "tank",
                    row.id,
                    "minLevel",
                    fromDisplay(parseFloat(v), "length", sys),
                  )
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-initialLevel`}
                display={
                  isPendingRow
                    ? ""
                    : formatQtyRaw(row.initialLevel, "length", sys)
                }
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`tank:${row.id}:initialLevel`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch(
                    "tank",
                    row.id,
                    "initialLevel",
                    fromDisplay(parseFloat(v), "length", sys),
                  )
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-maxLevel`}
                display={
                  isPendingRow ? "" : formatQtyRaw(row.maxLevel, "length", sys)
                }
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-secondary)" }}
                isPending={pendingKeys.has(`tank:${row.id}:maxLevel`)}
                inputType="number"
                min={0}
                onCommit={(v) =>
                  onPatch(
                    "tank",
                    row.id,
                    "maxLevel",
                    fromDisplay(parseFloat(v), "length", sys),
                  )
                }
              />
              <EditableCell
                key={`${discardGen}-${row.id}-diameter`}
                display={
                  isPendingRow
                    ? ""
                    : row.diameter != null
                      ? formatQtyRaw(row.diameter, "length", sys)
                      : "—"
                }
                placeholder={isPendingRow || row.diameter == null}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`tank:${row.id}:diameter`)}
                inputType="number"
                min={0}
                onCommit={(v) => {
                  const n = parseFloat(v);
                  if (!Number.isNaN(n))
                    onPatch(
                      "tank",
                      row.id,
                      "diameter",
                      fromDisplay(n, "length", sys),
                    );
                }}
              />
              <EditableCell
                key={`${discardGen}-${row.id}-volumeCurve`}
                display={isPendingRow ? "" : (row.volumeCurve ?? "—")}
                value={isPendingRow ? "" : (row.volumeCurve ?? "")}
                placeholder={isPendingRow || row.volumeCurve == null}
                isPending={pendingKeys.has(`tank:${row.id}:volumeCurve`)}
                onCommit={(v) => onPatch("tank", row.id, "volumeCurve", v)}
              />
              <EditableCell
                key={`${discardGen}-${row.id}-x`}
                display={isPendingRow ? "" : String(row.x)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`tank:${row.id}:x`)}
                inputType="number"
                onCommit={(v) => onPatch("tank", row.id, "x", parseFloat(v))}
              />
              <EditableCell
                key={`${discardGen}-${row.id}-y`}
                display={isPendingRow ? "" : String(row.y)}
                placeholder={isPendingRow}
                align="right"
                style={{ color: "var(--text-primary)" }}
                isPending={pendingKeys.has(`tank:${row.id}:y`)}
                inputType="number"
                onCommit={(v) => onPatch("tank", row.id, "y", parseFloat(v))}
              />
            </tr>
          );
        })}
        <VirtualSpacerRow height={paddingBottom} colSpan={COL_COUNT} />
      </tbody>
    </table>
  );
}
