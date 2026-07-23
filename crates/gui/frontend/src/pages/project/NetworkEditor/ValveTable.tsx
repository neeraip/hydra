import type React from "react";
import type { ValveRow } from "../../../hooks";
import {
  EditableCell,
  RefInputCell,
  RefOptionsDatalist,
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "./TablePrimitives";
import { shouldUseRefDatalist } from "./tableSearch";

export const VALVE_TYPES = ["PRV", "PSV", "FCV", "TCV", "GPV", "PBV", "PCV"];

const COL_COUNT = 6;

/** Single shared datalist id for every node-reference input in this table. */
const NODE_LIST_ID = "valve-node-options";

export function ValveTable({
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
}: {
  rows: ValveRow[];
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
  // Ref inputs only exist on pending (unsaved) rows, so the shared datalist
  // is only mounted while at least one pending row exists. `pendingRowIds` is
  // scoped to this table's element kind (see ElementsEditor), so pending rows
  // of other kinds do not mount it. Past the datalist size threshold no list
  // id is attached and inputs fall back to plain text + validation-on-blur
  // (see RefOptionsDatalist).
  const hasPendingRows = (pendingRowIds?.size ?? 0) > 0;
  const nodeListId = shouldUseRefDatalist(nodeOptions.length)
    ? NODE_LIST_ID
    : undefined;

  return (
    <>
      {hasPendingRows && (
        <RefOptionsDatalist id={NODE_LIST_ID} options={nodeOptions} />
      )}
      <table
        style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}
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
              field="valveType"
              label="Type"
              sortField={sortField}
              sortAsc={sortAsc}
              onSort={onSort}
            />
            <SortTh
              field="diameter"
              label="Ø (mm)"
              sortField={sortField}
              sortAsc={sortAsc}
              onSort={onSort}
              align="right"
            />
            <SortTh
              field="setting"
              label="Setting"
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
            const settingUnit =
              row.valveType === "FCV"
                ? "L/s"
                : row.valveType === "PRV" ||
                    row.valveType === "PSV" ||
                    row.valveType === "PBV"
                  ? "m"
                  : row.valveType === "TCV"
                    ? "K"
                    : null;
            const settingDisplay =
              row.setting != null
                ? `${row.setting}${settingUnit ? ` ${settingUnit}` : ""}`
                : row.curve
                  ? row.curve
                  : "—";
            const hasCurve = row.valveType === "GPV" || row.valveType === "PCV";
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
                    isPending={pendingKeys.has(`valve:${row.id}:id`)}
                    onCommit={(v) => onPatch("valve", row.id, "id", v.trim())}
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
                    listId={nodeListId}
                    isPending={pendingKeys.has(`valve:${row.id}:from`)}
                    onCommit={(v) => onPatch("valve", row.id, "from", v)}
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
                    listId={nodeListId}
                    isPending={pendingKeys.has(`valve:${row.id}:to`)}
                    onCommit={(v) => onPatch("valve", row.id, "to", v)}
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
                {/* Valve type — dropdown-style editable cell */}
                <td
                  style={{
                    ...tdStyle,
                    borderLeft: pendingKeys.has(`valve:${row.id}:valveType`)
                      ? "2px solid rgba(220,160,40,0.65)"
                      : undefined,
                  }}
                >
                  <select
                    value={row.valveType}
                    onChange={(e) =>
                      onPatch("valve", row.id, "valveType", e.target.value)
                    }
                    style={{
                      background: "transparent",
                      border: "none",
                      outline: "none",
                      fontSize: 12,
                      fontFamily: "var(--font-mono)",
                      color: "var(--text-primary)",
                      cursor: "pointer",
                      width: "100%",
                    }}
                  >
                    {VALVE_TYPES.map((t) => (
                      <option key={t} value={t}>
                        {t}
                      </option>
                    ))}
                  </select>
                </td>
                <EditableCell
                  key={`${discardGen}-${row.id}-diameter`}
                  display={isPendingRow ? "" : `${row.diameter} mm`}
                  placeholder={isPendingRow}
                  align="right"
                  style={{ color: "var(--text-primary)" }}
                  isPending={pendingKeys.has(`valve:${row.id}:diameter`)}
                  inputType="number"
                  min={0.1}
                  onCommit={(v) =>
                    onPatch("valve", row.id, "diameter", parseFloat(v))
                  }
                />
                {hasCurve ? (
                  <EditableCell
                    key={`${discardGen}-${row.id}-valveCurve`}
                    display={isPendingRow ? "" : settingDisplay}
                    value={isPendingRow ? "" : (row.curve ?? "")}
                    placeholder={isPendingRow || !row.curve}
                    isPending={pendingKeys.has(`valve:${row.id}:valveCurve`)}
                    onCommit={(v) => onPatch("valve", row.id, "valveCurve", v)}
                  />
                ) : (
                  <EditableCell
                    key={`${discardGen}-${row.id}-valveSetting`}
                    display={isPendingRow ? "" : settingDisplay}
                    value={
                      isPendingRow
                        ? ""
                        : row.setting != null
                          ? String(row.setting)
                          : ""
                    }
                    placeholder={isPendingRow || row.setting == null}
                    align="right"
                    style={{ color: "var(--text-primary)" }}
                    isPending={pendingKeys.has(`valve:${row.id}:valveSetting`)}
                    inputType="number"
                    min={0}
                    onCommit={(v) =>
                      onPatch("valve", row.id, "valveSetting", parseFloat(v))
                    }
                  />
                )}
              </tr>
            );
          })}
          <VirtualSpacerRow height={paddingBottom} colSpan={COL_COUNT} />
        </tbody>
      </table>
    </>
  );
}
