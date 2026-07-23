import type React from "react";
import type { PipeInitialStatus, PipeRow } from "../../../hooks";
import { formatQtyRaw, fromDisplay, useUnitSystem } from "../../../units";
import { PIPE_STATUS_OPTIONS, pipeStatusPatchValue } from "./pipeStatus";
import {
  EditableCell,
  RefInputCell,
  RefOptionsDatalist,
  SelectCell,
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "./TablePrimitives";
import { shouldUseRefDatalist } from "./tableSearch";

const COL_COUNT = 7;

/** Single shared datalist id for every node-reference input in this table. */
const NODE_LIST_ID = "pipe-node-options";

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
  scrollContainerRef,
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
            <SortTh
              field="initialStatus"
              label="Status"
              sortField={sortField}
              sortAsc={sortAsc}
              onSort={onSort}
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
                    listId={nodeListId}
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
                    listId={nodeListId}
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
                  display={
                    isPendingRow ? "" : formatQtyRaw(row.length, "length", sys)
                  }
                  placeholder={isPendingRow}
                  align="right"
                  style={{ color: "var(--text-primary)" }}
                  isPending={pendingKeys.has(`pipe:${row.id}:length`)}
                  inputType="number"
                  min={0}
                  onCommit={(v) =>
                    onPatch(
                      "pipe",
                      row.id,
                      "length",
                      fromDisplay(parseFloat(v), "length", sys),
                    )
                  }
                />
                <EditableCell
                  key={`${discardGen}-${row.id}-diameter`}
                  display={
                    isPendingRow
                      ? ""
                      : formatQtyRaw(row.diameter, "diameter", sys)
                  }
                  placeholder={isPendingRow}
                  align="right"
                  style={{ color: "var(--text-primary)" }}
                  isPending={pendingKeys.has(`pipe:${row.id}:diameter`)}
                  inputType="number"
                  min={0}
                  onCommit={(v) =>
                    onPatch(
                      "pipe",
                      row.id,
                      "diameter",
                      fromDisplay(parseFloat(v), "diameter", sys),
                    )
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
                <SelectCell
                  key={`${discardGen}-${row.id}-status`}
                  value={row.initialStatus}
                  options={PIPE_STATUS_OPTIONS}
                  isPending={pendingKeys.has(`pipe:${row.id}:status`)}
                  onCommit={(v) =>
                    onPatch(
                      "pipe",
                      row.id,
                      "status",
                      pipeStatusPatchValue(v as PipeInitialStatus),
                    )
                  }
                />
              </tr>
            );
          })}
          <VirtualSpacerRow height={paddingBottom} colSpan={COL_COUNT} />
        </tbody>
      </table>
    </>
  );
}
