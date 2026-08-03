import {
  MagnifyingGlassPlusIcon,
  TagIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import type { SimResultColumn } from "../../canvas/selection-context";
import type { Link, Node } from "../../hooks";
import { useLinks, useNodes, useRegions } from "../../hooks";
import { perfTrace } from "../../perfTrace";
import type { Region } from "../../types";
import { elementTypeBadge } from "../../types/elementTypes";
import { toDisplay, unitLabel, useUnitSystem } from "../../units";
import { MiddleTruncate } from "../ui/MiddleTruncate";
import { TypeBadge } from "../ui/TypeBadge";

// ── Sort / filter hook ────────────────────────────────────────────────────────

type SortDir = "asc" | "desc";

function hasNodeCoordinates(node: Node): boolean {
  return !(node.x === 0 && node.y === 0);
}

/**
 * Case-insensitive substring match over `searchKeys`.
 *
 * Shared by the tab lists and by the tab-strip counts: computing "how many
 * match" separately from "which ones show" is how a badge ends up claiming 12
 * results over a list of 9.
 */
export function matchesQuery<T>(
  item: T,
  searchKeys: (keyof T)[],
  loweredQuery: string,
): boolean {
  return searchKeys.some((k) =>
    String((item as Record<string, unknown>)[k as string] ?? "")
      .toLowerCase()
      .includes(loweredQuery),
  );
}

function useSortedFiltered<T>(
  items: T[],
  query: string,
  searchKeys: (keyof T)[],
  traceTab: "nodes" | "links",
): [T[], string | null, SortDir, (col: string) => void] {
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const lastTraceKeyRef = useRef<string>("");

  // Tri-state: first click sorts ascending, second descending, third clears
  // the sort entirely — restoring the network's natural (file) order, which
  // is not necessarily ID order.
  function toggleSort(col: string) {
    if (sortCol !== col) {
      setSortCol(col);
      setSortDir("asc");
    } else if (sortDir === "asc") {
      setSortDir("desc");
    } else {
      setSortCol(null);
      setSortDir("asc");
    }
  }

  const result = useMemo(() => {
    const t0 = performance.now();
    const q = query.toLowerCase();
    let arr = q
      ? items.filter((item) => matchesQuery(item, searchKeys, q))
      : items;
    if (sortCol) {
      // "resultValues.<id>" reaches into the engine-generic value bag;
      // every other column is a direct field.
      const RESULT_PREFIX = "resultValues.";
      const get = (o: Record<string, unknown>): unknown =>
        sortCol.startsWith(RESULT_PREFIX)
          ? (o.resultValues as Record<string, unknown> | undefined)?.[
              sortCol.slice(RESULT_PREFIX.length)
            ]
          : o[sortCol];
      arr = [...arr].sort((a, b) => {
        const av = get(a as Record<string, unknown>) ?? "";
        const bv = get(b as Record<string, unknown>) ?? "";
        const cmp = av < bv ? -1 : av > bv ? 1 : 0;
        return sortDir === "asc" ? cmp : -cmp;
      });
    }

    const deriveMs = performance.now() - t0;
    const shouldTrace =
      items.length > 0 &&
      deriveMs >= 2 &&
      (q.length > 0 || sortCol !== null || items.length > 1000);
    if (shouldTrace) {
      const traceKey = `${traceTab}:${items.length}:${arr.length}:${q}:${sortCol ?? "none"}:${sortDir}`;
      if (lastTraceKeyRef.current !== traceKey) {
        lastTraceKeyRef.current = traceKey;
        perfTrace("network-list-derive", deriveMs, {
          tab: traceTab,
          inputCount: items.length,
          resultCount: arr.length,
          queryLen: q.length,
          sortCol: sortCol ?? "none",
          sortDir,
        });
      }
    }

    return arr;
  }, [items, query, searchKeys, sortCol, sortDir, traceTab]);

  return [result, sortCol, sortDir, toggleSort];
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(id);
  }, [delayMs, value]);

  return debounced;
}

// ── Shared table styles ───────────────────────────────────────────────────────

const TH: React.CSSProperties = {
  padding: "5px 8px",
  textAlign: "left",
  fontSize: "var(--text-xs)",
  fontWeight: 600,
  letterSpacing: "0.05em",
  textTransform: "uppercase",
  color: "var(--text-tertiary)",
  borderBottom: "1px solid var(--border)",
  whiteSpace: "nowrap",
  cursor: "pointer",
  userSelect: "none",
  position: "sticky",
  top: 0,
  background: "var(--bg-panel)",
  zIndex: 1,
};

/** Width of the element-type badge column: the badge is ~20px wide (18px
 * minimum, a little more for the two-character "Pu"), plus 4px each side. The
 * shared TD padding of 8px would otherwise make the column twice the width of
 * the thing in it. */
const BADGE_COL_WIDTH = 28;

/** Cell padding for that column — the shared horizontal padding, halved. */
const BADGE_CELL_PADDING = "4px 4px";

const TD: React.CSSProperties = {
  padding: "4px 8px",
  fontSize: "var(--text-sm)",
  borderBottom: "1px solid rgba(255,255,255,0.04)",
  whiteSpace: "nowrap",
  overflow: "hidden",
  textOverflow: "ellipsis",
  maxWidth: 110,
  // Rows are click targets (select/locate an element); dragging across
  // them must not highlight cell text.
  userSelect: "none",
};

/**
 * Element-type cell for the nodes/links tables.
 *
 * The badge is centred as a *box* rather than aligned on the row's text
 * baseline. `TypeBadge` is an inline-flex box, so inline layout aligns it by
 * the baseline of the letter inside it — and that letter's baseline sits below
 * the badge's own centre, which dragged the whole badge about a pixel below the
 * centre of the row. A block-level flex container has no baseline to align to,
 * and `vertical-align: middle` on the cell centres that block in the row, so
 * the result no longer depends on font metrics.
 */
function BadgeCell({ type }: { type: string }) {
  return (
    <td style={{ ...TD, padding: BADGE_CELL_PADDING, verticalAlign: "middle" }}>
      <span
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <TypeBadge type={type} />
      </span>
    </td>
  );
}

/** Magnitude-aware value cell for the generic result columns — drainage
 * peaks can be 0.03 cfs, which a fixed `toFixed(1)` would print as 0.0. */
function fmtResultValue(v: number): string {
  const a = Math.abs(v);
  if (a >= 1000) return Math.round(v).toLocaleString();
  if (a >= 10) return v.toFixed(1);
  return v.toFixed(2);
}

/** Sortable header + cell pair for one engine-generic result column. */
function GenericResultHeader({
  column,
  sortCol,
  sortDir,
  onToggleSort,
}: {
  column: SimResultColumn;
  sortCol: string | null;
  sortDir: SortDir;
  onToggleSort: (col: string) => void;
}) {
  const sortKey = `resultValues.${column.key}`;
  return (
    <th
      style={TH}
      onClick={() => onToggleSort(sortKey)}
      data-tooltip={
        column.unit ? `${column.label} (${column.unit})` : column.label
      }
      data-tooltip-pos="bottom"
    >
      {column.label}
      <SortIndicator col={sortKey} sortCol={sortCol} sortDir={sortDir} />
    </th>
  );
}

function GenericResultCell({ value }: { value: number | null | undefined }) {
  return (
    <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
      {value != null && Number.isFinite(value) ? fmtResultValue(value) : "—"}
    </td>
  );
}

function SortIndicator({
  col,
  sortCol,
  sortDir,
}: {
  col: string;
  sortCol: string | null;
  sortDir: SortDir;
}) {
  if (sortCol !== col)
    return <span style={{ opacity: 0.25, marginLeft: 3 }}>↕</span>;
  return (
    <span style={{ marginLeft: 3, color: "var(--accent)" }}>
      {sortDir === "asc" ? "↑" : "↓"}
    </span>
  );
}

// ── Nodes tab ────────────────────────────────────────────────────────────────

const NODE_SEARCH_KEYS: (keyof Node)[] = ["id", "type"];

function NodesTab({
  query,
  nodes,
  onSelect,
  onZoomTo,
  activeId,
  resultColumns = [],
}: {
  query: string;
  nodes: Node[];
  onSelect: (id: string) => void;
  onZoomTo?: (id: string) => void;
  activeId?: string | null;
  resultColumns?: SimResultColumn[];
}) {
  const sys = useUnitSystem();
  const hasResults = nodes.some((n) => n.pressure != null);
  // Engine-generic result columns: headers from the engine's catalog,
  // values from `resultValues` (merged by CanvasView). Shown once any
  // element carries a value.
  const genericColumns = nodes.some((n) => n.resultValues != null)
    ? resultColumns
    : [];
  // Attribute columns render only when the snapshot carries the attribute —
  // engines whose snapshot is geometry-only (v4) get ID + badge, not a
  // column of dashes.
  const hasAttrs = nodes.some(
    (n) => n.elevation != null || n.baseDemand != null,
  );
  const [rows, sortCol, sortDir, toggleSort] = useSortedFiltered(
    nodes,
    query,
    NODE_SEARCH_KEYS,
    "nodes",
  );
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 27,
    overscan: 12,
  });
  const virtualRows = rowVirtualizer.getVirtualItems();
  const padTop = virtualRows.length > 0 ? virtualRows[0].start : 0;
  const padBottom =
    virtualRows.length > 0
      ? rowVirtualizer.getTotalSize() - virtualRows[virtualRows.length - 1].end
      : 0;
  const nodeColSpan =
    2 +
    (hasAttrs ? 2 : 0) +
    (hasResults ? 1 : 0) +
    genericColumns.length +
    (onZoomTo ? 1 : 0);

  return (
    <div ref={scrollRef} style={{ overflow: "auto", flex: 1 }}>
      <table
        style={{
          width: "100%",
          minWidth: "100%",
          borderCollapse: "collapse",
          tableLayout: "fixed",
          // Rows are click targets; WKWebView needs the -webkit- form for
          // user-select, and table-level scope covers every cell style.
          userSelect: "none",
          WebkitUserSelect: "none",
        }}
      >
        {/* Fixed table layout for scroll-stable, measurement-free columns
            (the list is virtualized); only the intrinsically-sized columns
            are pinned — the ID/data columns share the remaining rail width. */}
        <colgroup>
          <col style={{ width: BADGE_COL_WIDTH }} />
          <col />
          {hasAttrs && <col />}
          {hasAttrs && <col />}
          {hasResults && <col />}
          {genericColumns.map((c) => (
            <col key={c.key} />
          ))}
          {onZoomTo && <col style={{ width: 22 }} />}
        </colgroup>
        <thead>
          <tr>
            {/* Badge column, first to match the cells. A tag glyph rather
                than a word: the column is sized to the badge, and no label
                fits — "Type" is wider than the column, and the single letter
                "T" would sit directly above a column of J/R/T/P/Pu/V, where
                T already means Tank. The tooltip carries the full name. */}
            <th
              style={{
                ...TH,
                textAlign: "center",
                padding: "5px 4px",
                verticalAlign: "middle",
              }}
              onClick={() => toggleSort("type")}
              data-tooltip="Element type — click to sort"
              data-tooltip-pos="bottom"
            >
              {/* Block-level flex, not inline-flex: the glyph is an SVG box and
                  the sort arrow is a text character, so an inline box aligned
                  one to the line box and the other to the baseline. Going
                  block-level removes the line box from the question entirely —
                  the cell's `vertical-align: middle` then centres this row of
                  content, matching how BadgeCell centres the badges below. */}
              <span
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  lineHeight: 1,
                }}
              >
                <TagIcon
                  style={{ width: 10, height: 10 }}
                  aria-label="Element type"
                />
                <SortIndicator col="type" sortCol={sortCol} sortDir={sortDir} />
              </span>
            </th>
            {(["id", "elevation", "baseDemand"] as const)
              .filter((col) => col === "id" || hasAttrs)
              .map((col) => {
                const meta = {
                  id: { label: "ID", tip: "Node ID" },
                  elevation: {
                    label: "Elev",
                    tip: `Elevation (${unitLabel("elevation", sys)})`,
                  },
                  baseDemand: {
                    label: "Dem",
                    tip: `Base demand (${unitLabel("demand", sys)})`,
                  },
                }[col];
                return (
                  <th
                    key={col}
                    style={TH}
                    onClick={() => toggleSort(col)}
                    data-tooltip={meta.tip}
                    data-tooltip-pos="bottom"
                  >
                    {meta.label}
                    <SortIndicator
                      col={col}
                      sortCol={sortCol}
                      sortDir={sortDir}
                    />
                  </th>
                );
              })}
            {hasResults && (
              <th
                style={TH}
                onClick={() => toggleSort("pressure")}
                data-tooltip={`Pressure (${unitLabel("pressure", sys)})`}
                data-tooltip-pos="bottom"
              >
                Pres
                <SortIndicator
                  col="pressure"
                  sortCol={sortCol}
                  sortDir={sortDir}
                />
              </th>
            )}
            {genericColumns.map((c) => (
              <GenericResultHeader
                key={c.key}
                column={c}
                sortCol={sortCol}
                sortDir={sortDir}
                onToggleSort={toggleSort}
              />
            ))}
            {onZoomTo && <th style={TH} />}
          </tr>
        </thead>
        <tbody>
          {padTop > 0 && (
            <tr>
              <td
                colSpan={nodeColSpan}
                style={{ height: padTop, padding: 0, borderBottom: "none" }}
              />
            </tr>
          )}
          {virtualRows.map((virtualRow) => {
            const node = rows[virtualRow.index];
            const isActive = node.id === activeId;
            const canZoomTo = hasNodeCoordinates(node);
            return (
              <tr
                key={node.id}
                onClick={() => onSelect(node.id)}
                style={{
                  cursor: "pointer",
                  background: isActive ? "rgba(79,142,247,0.14)" : undefined,
                  outline: isActive
                    ? "1px solid rgba(79,142,247,0.3)"
                    : undefined,
                  outlineOffset: "-1px",
                }}
                onMouseEnter={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "rgba(255,255,255,0.04)";
                }}
                onMouseLeave={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "transparent";
                }}
              >
                <BadgeCell type={node.type} />
                <td
                  style={{
                    ...TD,
                    color: "var(--accent)",
                    fontWeight: 500,
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  <MiddleTruncate text={node.id} />
                </td>
                {hasAttrs && (
                  <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                    {node.elevation != null
                      ? toDisplay(node.elevation, "elevation", sys).toFixed(1)
                      : "—"}
                  </td>
                )}
                {hasAttrs && (
                  <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                    {node.baseDemand != null
                      ? toDisplay(node.baseDemand, "demand", sys).toFixed(
                          sys === "si" ? 2 : 1,
                        )
                      : "—"}
                  </td>
                )}
                {hasResults && (
                  <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                    {node.pressure != null
                      ? toDisplay(node.pressure, "pressure", sys).toFixed(1)
                      : "—"}
                  </td>
                )}
                {genericColumns.map((c) => (
                  <GenericResultCell
                    key={c.key}
                    value={node.resultValues?.[c.key]}
                  />
                ))}
                {onZoomTo && (
                  <td
                    style={{
                      ...TD,
                      padding: "4px 4px 4px 0",
                      textAlign: "right",
                    }}
                  >
                    <button
                      type="button"
                      disabled={!canZoomTo}
                      onClick={(e) => {
                        e.stopPropagation();
                        if (!canZoomTo) return;
                        onZoomTo(node.id);
                      }}
                      style={{
                        background: "transparent",
                        border: "none",
                        padding: 2,
                        cursor: canZoomTo ? "pointer" : "not-allowed",
                        color: "var(--text-tertiary)",
                        display: "inline-flex",
                        borderRadius: 3,
                        lineHeight: 0,
                        opacity: canZoomTo ? 1 : 0.45,
                      }}
                      onMouseEnter={(e) => {
                        if (!canZoomTo) return;
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--accent)";
                      }}
                      onMouseLeave={(e) => {
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--text-tertiary)";
                      }}
                    >
                      <MagnifyingGlassPlusIcon
                        style={{ width: 11, height: 11 }}
                      />
                    </button>
                  </td>
                )}
              </tr>
            );
          })}
          {padBottom > 0 && (
            <tr>
              <td
                colSpan={nodeColSpan}
                style={{ height: padBottom, padding: 0, borderBottom: "none" }}
              />
            </tr>
          )}
        </tbody>
      </table>
      {rows.length === 0 && (
        <div
          style={{
            padding: 14,
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            fontStyle: "italic",
          }}
        >
          No nodes match.
        </div>
      )}
    </div>
  );
}

// ── Links tab ────────────────────────────────────────────────────────────────

// Hydra OUT-file status codes (status_to_f32 in out_writer.rs)
const STATUS_COLOR: Record<number, string> = {
  3: "var(--status-success)", // Open
  2: "var(--status-error)", // Closed
  0: "var(--status-error)", // XHead (pump overloaded)
  1: "var(--status-error)", // TempClosed
  4: "#d4a017", // Active (control valve)
  6: "#d4a017", // XFcv
  7: "#d4a017", // XPressure
};

const STATUS_LABEL: Record<number, string> = {
  3: "Open",
  2: "Closed",
  0: "Closed (XHead)",
  1: "Temp Closed",
  4: "Active",
  6: "Active (XFcv)",
  7: "Active (XPressure)",
};

const LINK_SEARCH_KEYS: (keyof Link)[] = ["id", "type", "fromId", "toId"];

function LinksTab({
  query,
  links,
  zoomableNodeIds,
  onSelect,
  onZoomTo,
  activeId,
  resultColumns = [],
}: {
  query: string;
  links: Link[];
  zoomableNodeIds: Set<string>;
  onSelect: (id: string) => void;
  onZoomTo?: (id: string) => void;
  activeId?: string | null;
  resultColumns?: SimResultColumn[];
}) {
  const sys = useUnitSystem();
  const hasResults = links.some((l) => l.flow != null);
  const genericColumns = links.some((l) => l.resultValues != null)
    ? resultColumns
    : [];
  // Same rule as the nodes table: attribute columns only when the snapshot
  // carries the attribute (v4 snapshots are geometry-only).
  const hasAttrs = links.some(
    (l) => l.diameter != null || l.status != null || l.initialStatus != null,
  );
  const [rows, sortCol, sortDir, toggleSort] = useSortedFiltered(
    links,
    query,
    LINK_SEARCH_KEYS,
    "links",
  );
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 27,
    overscan: 12,
  });
  const virtualRows = rowVirtualizer.getVirtualItems();
  const padTop = virtualRows.length > 0 ? virtualRows[0].start : 0;
  const padBottom =
    virtualRows.length > 0
      ? rowVirtualizer.getTotalSize() - virtualRows[virtualRows.length - 1].end
      : 0;
  const linkColSpan =
    2 +
    (hasAttrs ? 2 : 0) +
    (hasResults ? 1 : 0) +
    genericColumns.length +
    (onZoomTo ? 1 : 0);

  return (
    <div ref={scrollRef} style={{ overflow: "auto", flex: 1 }}>
      <table
        style={{
          width: "100%",
          minWidth: "100%",
          borderCollapse: "collapse",
          tableLayout: "fixed",
          // Rows are click targets; WKWebView needs the -webkit- form for
          // user-select, and table-level scope covers every cell style.
          userSelect: "none",
          WebkitUserSelect: "none",
        }}
      >
        {/* Same scheme as the nodes table: fixed layout, pinned narrow
            columns, flexing ID/data columns. */}
        <colgroup>
          <col style={{ width: BADGE_COL_WIDTH }} />
          <col />
          {hasAttrs && <col style={{ width: 36 }} />}
          {hasAttrs && <col />}
          {hasResults && <col />}
          {genericColumns.map((c) => (
            <col key={c.key} />
          ))}
          {onZoomTo && <col style={{ width: 22 }} />}
        </colgroup>
        <thead>
          <tr>
            {/* Badge column, first to match the cells. A tag glyph rather
                than a word: the column is sized to the badge, and no label
                fits — "Type" is wider than the column, and the single letter
                "T" would sit directly above a column of J/R/T/P/Pu/V, where
                T already means Tank. The tooltip carries the full name. */}
            <th
              style={{
                ...TH,
                textAlign: "center",
                padding: "5px 4px",
                verticalAlign: "middle",
              }}
              onClick={() => toggleSort("type")}
              data-tooltip="Element type — click to sort"
              data-tooltip-pos="bottom"
            >
              {/* Flex rather than inline: the glyph is an SVG box and the
                  sort arrow is a text character, so leaving them inline
                  aligned one to the line box and the other to the baseline,
                  and the icon rode high. */}
              <span
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <TagIcon
                  style={{ width: 10, height: 10 }}
                  aria-label="Element type"
                />
                <SortIndicator col="type" sortCol={sortCol} sortDir={sortDir} />
              </span>
            </th>
            {(["id", "status", "diameter"] as const)
              .filter((col) => col === "id" || hasAttrs)
              .map((col) => {
                const meta = {
                  id: { label: "ID", tip: "Link ID" },
                  status: { label: "St.", tip: "Status" },
                  diameter: {
                    label: "Ø",
                    tip: `Diameter (${unitLabel("diameter", sys)})`,
                  },
                }[col];
                return (
                  <th
                    key={col}
                    style={TH}
                    onClick={() => toggleSort(col)}
                    data-tooltip={meta.tip}
                    data-tooltip-pos="bottom"
                  >
                    {meta.label}
                    <SortIndicator
                      col={col}
                      sortCol={sortCol}
                      sortDir={sortDir}
                    />
                  </th>
                );
              })}
            {hasResults && (
              <th
                style={TH}
                onClick={() => toggleSort("flow")}
                data-tooltip={`Flow (${unitLabel("flow", sys)})`}
                data-tooltip-pos="bottom"
              >
                Flow
                <SortIndicator col="flow" sortCol={sortCol} sortDir={sortDir} />
              </th>
            )}
            {genericColumns.map((c) => (
              <GenericResultHeader
                key={c.key}
                column={c}
                sortCol={sortCol}
                sortDir={sortDir}
                onToggleSort={toggleSort}
              />
            ))}
            {onZoomTo && <th style={TH} />}
          </tr>
        </thead>
        <tbody>
          {padTop > 0 && (
            <tr>
              <td
                colSpan={linkColSpan}
                style={{ height: padTop, padding: 0, borderBottom: "none" }}
              />
            </tr>
          )}
          {virtualRows.map((virtualRow) => {
            const link = rows[virtualRow.index];
            const isActive = link.id === activeId;
            const canZoomTo =
              zoomableNodeIds.has(link.fromId) &&
              zoomableNodeIds.has(link.toId);
            return (
              <tr
                key={link.id}
                onClick={() => onSelect(link.id)}
                style={{
                  cursor: "pointer",
                  background: isActive ? "rgba(79,142,247,0.14)" : undefined,
                  outline: isActive
                    ? "1px solid rgba(79,142,247,0.3)"
                    : undefined,
                  outlineOffset: "-1px",
                }}
                onMouseEnter={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "rgba(255,255,255,0.04)";
                }}
                onMouseLeave={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "transparent";
                }}
              >
                <BadgeCell type={link.type} />
                <td
                  style={{
                    ...TD,
                    color: "var(--accent)",
                    fontWeight: 500,
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  <MiddleTruncate text={link.id} />
                </td>
                {hasAttrs && (
                  <td style={TD}>
                    {link.status != null ? (
                      <span
                        data-tooltip={STATUS_LABEL[link.status] ?? "Unknown"}
                        style={{
                          display: "inline-block",
                          width: 7,
                          height: 7,
                          borderRadius: "50%",
                          background:
                            STATUS_COLOR[link.status] ?? "var(--text-tertiary)",
                        }}
                      />
                    ) : (
                      <span style={{ color: "var(--text-tertiary)" }}>—</span>
                    )}
                  </td>
                )}
                {hasAttrs && (
                  <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                    {link.diameter != null
                      ? toDisplay(link.diameter, "diameter", sys).toFixed(
                          sys === "si" ? 0 : 1,
                        )
                      : "—"}
                  </td>
                )}
                {hasResults && (
                  <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                    {link.flow != null
                      ? toDisplay(link.flow, "flow", sys).toFixed(
                          sys === "si" ? 2 : 1,
                        )
                      : "—"}
                  </td>
                )}
                {genericColumns.map((c) => (
                  <GenericResultCell
                    key={c.key}
                    value={link.resultValues?.[c.key]}
                  />
                ))}
                {onZoomTo && (
                  <td
                    style={{
                      ...TD,
                      padding: "4px 4px 4px 0",
                      textAlign: "right",
                    }}
                  >
                    <button
                      type="button"
                      disabled={!canZoomTo}
                      onClick={(e) => {
                        e.stopPropagation();
                        if (!canZoomTo) return;
                        onZoomTo(link.id);
                      }}
                      style={{
                        background: "transparent",
                        border: "none",
                        padding: 2,
                        cursor: canZoomTo ? "pointer" : "not-allowed",
                        color: "var(--text-tertiary)",
                        display: "inline-flex",
                        borderRadius: 3,
                        lineHeight: 0,
                        opacity: canZoomTo ? 1 : 0.45,
                      }}
                      onMouseEnter={(e) => {
                        if (!canZoomTo) return;
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--accent)";
                      }}
                      onMouseLeave={(e) => {
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--text-tertiary)";
                      }}
                    >
                      <MagnifyingGlassPlusIcon
                        style={{ width: 11, height: 11 }}
                      />
                    </button>
                  </td>
                )}
              </tr>
            );
          })}
          {padBottom > 0 && (
            <tr>
              <td
                colSpan={linkColSpan}
                style={{ height: padBottom, padding: 0, borderBottom: "none" }}
              />
            </tr>
          )}
        </tbody>
      </table>
      {rows.length === 0 && (
        <div
          style={{
            padding: 14,
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            fontStyle: "italic",
          }}
        >
          No links match.
        </div>
      )}
    </div>
  );
}

// ── Patterns tab ──────────────────────────────────────────────────────────────

/** Patterns deliberately absent: they have no position, so nothing about them
 * appears on the canvas this panel accompanies, and the Editor already has a
 * Patterns section where they can actually be edited. */

// ── Subcatchments tab ─────────────────────────────────────────────────────────

/** Areal elements, present only for engines that have them. Read-only rows —
 * region selection on the canvas arrives with the region inspector. */
function SubcatchmentsTab({
  query,
  regions,
}: {
  query: string;
  regions: Region[];
}) {
  const q = query.trim().toLowerCase();
  const shown = q
    ? regions.filter((r) => r.id.toLowerCase().includes(q))
    : regions;
  if (shown.length === 0) {
    return (
      <div
        style={{
          padding: 14,
          fontSize: "var(--text-md)",
          color: "var(--text-tertiary)",
        }}
      >
        {q ? "No subcatchments match." : "No subcatchments."}
      </div>
    );
  }
  return (
    <div style={{ overflowY: "auto", flex: 1 }}>
      {shown.map((r) => {
        const badge = elementTypeBadge(r.type);
        return (
          <div
            key={r.id}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "6px 14px",
              fontSize: "var(--text-md)",
              color: "var(--text-primary)",
              borderBottom: "1px solid var(--border)",
            }}
          >
            <span
              style={{
                width: 16,
                height: 16,
                borderRadius: 4,
                background: badge.color,
                color: "#fff",
                fontSize: 9,
                fontWeight: 700,
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
              }}
            >
              {badge.label}
            </span>
            <span
              style={{
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {r.id}
            </span>
          </div>
        );
      })}
    </div>
  );
}
type HomeTab = "nodes" | "links" | "subcatchments";

interface Props {
  /** When omitted the close button is hidden (e.g. when rendered inside the rail). */
  onClose?: () => void;
  onSelectNode: (id: string) => void;
  onSelectLink: (id: string) => void;
  /** When provided, pattern cards are clickable and navigate to the editor. */
  /** Override the internal `useNodes()` call (e.g. pass merged sim-result nodes). */
  nodes?: Node[];
  /** Override the internal `useLinks()` call (e.g. pass merged sim-result links). */
  links?: Link[];
  /** Highlight this node id in the nodes list (e.g. the currently inspected element). */
  activeNodeId?: string | null;
  /** Highlight this link id in the links list (e.g. the currently inspected element). */
  activeLinkId?: string | null;
  /** When provided, each node row shows a zoom icon that triggers this callback. */
  onZoomToNode?: (id: string) => void;
  /** When provided, each link row shows a zoom icon that triggers this callback. */
  onZoomToLink?: (id: string) => void;
  /** Generic result-column headers (engines whose values ride on
   * `resultValues`); absent for wds, whose pressure and flow columns are
   * built in. */
  nodeResultColumns?: SimResultColumn[];
  linkResultColumns?: SimResultColumn[];
  /**
   * When true the panel renders inline (fills its container) rather than as an
   * absolutely-positioned overlay. Use this when hosting inside the secondary rail.
   */
  embedded?: boolean;
}

export function NetworkInspectorHome({
  onClose,
  onSelectNode,
  onSelectLink,
  nodes: nodesProp,
  links: linksProp,
  activeNodeId,
  activeLinkId,
  onZoomToNode,
  onZoomToLink,
  nodeResultColumns,
  linkResultColumns,
  embedded,
}: Props) {
  const internalNodes = useNodes();
  const internalLinks = useLinks();
  const allNodes = nodesProp ?? internalNodes;
  const allLinks = linksProp ?? internalLinks;
  // Derived from the base network rather than `allNodes`: sim-merged node
  // arrays change identity on every timeline scrub, but x/y never do — using
  // `internalNodes` keeps this Set stable across scrubs.
  const zoomableNodeIds = useMemo(
    () => new Set(internalNodes.filter(hasNodeCoordinates).map((n) => n.id)),
    [internalNodes],
  );

  const regions = useRegions();
  const [tab, setTab] = useState<HomeTab>("nodes");
  const [queryInput, setQueryInput] = useState("");
  const query = useDebouncedValue(queryInput, 120);

  // While searching, the badges count *matches* rather than totals — that is
  // what tells you a query typed on the Nodes tab found something in Links.
  // Without it the only feedback is an empty list, which reads as "no results"
  // rather than "no results here".
  //
  // Computed from the debounced query, so a keystroke does not trigger a pass
  // over both collections; the same `matchesQuery` the lists use, so a badge
  // cannot disagree with the rows under it.
  const counts: Record<HomeTab, number> = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q)
      return {
        nodes: allNodes.length,
        links: allLinks.length,
        subcatchments: regions.length,
      };
    let nodes = 0;
    for (const n of allNodes) {
      if (matchesQuery(n, NODE_SEARCH_KEYS, q)) nodes++;
    }
    let links = 0;
    for (const l of allLinks) {
      if (matchesQuery(l, LINK_SEARCH_KEYS, q)) links++;
    }
    let subcatchments = 0;
    for (const r of regions) {
      if (r.id.toLowerCase().includes(q)) subcatchments++;
    }
    return { nodes, links, subcatchments };
  }, [allNodes, allLinks, regions, query]);
  const searching = query.trim().length > 0;
  const totals: Record<HomeTab, number> = {
    nodes: allNodes.length,
    links: allLinks.length,
    subcatchments: regions.length,
  };

  return (
    <div
      className="inspector-panel"
      style={
        embedded
          ? {
              flex: 1,
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              minHeight: 0,
              width: "100%",
            }
          : {
              position: "absolute",
              right: 0,
              top: 0,
              bottom: 0,
              zIndex: 30,
              display: "flex",
              flexDirection: "column",
            }
      }
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "10px 12px",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
        }}
      >
        {/* Title does not flex: the search field takes the slack instead, so a
            narrow rail shrinks the field rather than squeezing the label into
            an ellipsis. Element counts are deliberately absent — the tab strip
            below already carries one per tab. */}
        <div
          style={{
            fontSize: "var(--text-lg)",
            fontWeight: 600,
            color: "var(--text-primary)",
            flexShrink: 0,
          }}
        >
          Network
        </div>
        <input
          value={queryInput}
          onChange={(e) => setQueryInput(e.target.value)}
          placeholder="Search…"
          aria-label="Search network elements"
          style={{
            flex: 1,
            minWidth: 0,
            padding: "4px 8px",
            borderRadius: 6,
            border: "1px solid var(--border)",
            background: "rgba(255,255,255,0.04)",
            color: "var(--text-primary)",
            fontSize: "var(--text-sm)",
            outline: "none",
            boxSizing: "border-box",
          }}
        />
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            flexShrink: 0,
          }}
        >
          {onClose && (
            <button
              type="button"
              onClick={onClose}
              data-tooltip="Close"
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-tertiary)",
                cursor: "pointer",
                padding: 4,
                lineHeight: 1,
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <XMarkIcon style={{ width: 14, height: 14 }} />
            </button>
          )}
        </div>
      </div>

      {/* Tab strip */}
      <div
        style={{
          display: "flex",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
          background: "var(--bg-rail)",
          marginTop: 8,
          overflowX: "auto",
          scrollbarWidth: "none",
        }}
      >
        {(regions.length > 0
          ? (["nodes", "links", "subcatchments"] as HomeTab[])
          : (["nodes", "links"] as HomeTab[])
        ).map((t) => {
          const active = t === tab;
          return (
            <button
              type="button"
              key={t}
              onClick={() => setTab(t)}
              className={`inspector-tab${active ? " active" : ""}`}
            >
              <span style={{ textTransform: "capitalize" }}>{t}</span>
              {/* While searching, an inactive tab holding matches is accented:
                  the whole point is to be noticed from the other tab. A tab
                  with none is dimmed to the disabled colour so "nothing here"
                  reads differently from "nothing anywhere". */}
              <span
                data-tooltip={
                  searching
                    ? `${counts[t]} of ${totals[t]} match`
                    : `${totals[t]} ${t}`
                }
                data-tooltip-pos="bottom"
                style={{
                  marginLeft: 4,
                  fontSize: "var(--text-xs)",
                  padding: "1px 4px",
                  borderRadius: 4,
                  background: active
                    ? "rgba(79,142,247,0.18)"
                    : searching && counts[t] > 0
                      ? "rgba(212,160,23,0.18)"
                      : "var(--bg-card)",
                  color: active
                    ? "var(--accent)"
                    : searching
                      ? counts[t] > 0
                        ? "#d4a017"
                        : "var(--text-disabled)"
                      : "var(--text-tertiary)",
                  fontFamily: "var(--font-mono)",
                }}
              >
                {counts[t]}
              </span>
            </button>
          );
        })}
      </div>

      {/* Tab body */}
      {tab === "nodes" && (
        <NodesTab
          query={query}
          nodes={allNodes}
          onSelect={onSelectNode}
          onZoomTo={onZoomToNode}
          activeId={activeNodeId}
          resultColumns={nodeResultColumns}
        />
      )}
      {tab === "subcatchments" && (
        <SubcatchmentsTab query={query} regions={regions} />
      )}
      {tab === "links" && (
        <LinksTab
          query={query}
          links={allLinks}
          zoomableNodeIds={zoomableNodeIds}
          onSelect={onSelectLink}
          onZoomTo={onZoomToLink}
          activeId={activeLinkId}
          resultColumns={linkResultColumns}
        />
      )}
    </div>
  );
}
