// ── The network finder ────────────────────────────────────────────────────────
//
// The rail beside the canvas answers two questions: *where is this element*
// and *what am I looking at*. It is not a spreadsheet — the Elements view
// is, with every property a kind declares and the full result set. Trying
// to be a second one inside 280 pixels is what gave this panel three tabs,
// a three-column cap, and a row you could not find.
//
// So it is a finder:
//
//   · one flat list over every element, whatever its kind or class,
//     because someone hunting "J-401" does not know which tab it is in;
//   · search that ranks — an exact id first, then a prefix, then anything
//     that merely contains the text, including what an element connects to;
//   · kind chips instead of tabs, so browsing by kind still works and
//     works for all nine drainage kinds, not just two classes;
//   · selection that arrives from the canvas scrolls its row into view.
//
// Kinds, labels and result variables come from the engine catalogs, so this
// file names no kind and no engine.

import { EyeIcon, XMarkIcon } from "@heroicons/react/16/solid";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useActiveProject } from "../../AppContext";
import { useHoverActions } from "../../canvas/hover-context";
import type { SimResultColumn } from "../../canvas/selection-context";
import {
  useViewportActions,
  useViewportKey,
} from "../../canvas/viewport-context";
import type { Link, Node } from "../../hooks";
import { useElementKinds, useLinks, useNodes, useRegions } from "../../hooks";
import { perfTrace } from "../../perfTrace";
import { readTextScale } from "../../textScale";
import type { Region } from "../../types";
import { type UnitSystem, useUnitSystem } from "../../units";
import { TypeBadge } from "../ui/TypeBadge";
import { activeElement, activeKey, isActiveRow } from "./activeElement";
import {
  formatMeta,
  formatValue,
  NetworkListRow,
  type Row,
  unitOf,
} from "./NetworkListRow";
import { fitContent } from "./networkListFit";

/** Padding and border of a row — chrome, so it does not move with text. */
const ROW_CHROME = 12;
/** Line box of the id at text scale 1 (11px text). */
const ID_LINE_AT_SCALE_1 = 15;
/** Line box of the context line at text scale 1 (9px text). */
const CONTEXT_LINE_AT_SCALE_1 = 12;

/**
 * Height of one row of the network list.
 *
 * The virtualiser positions every row by this number, so a height that
 * does not account for what a row actually renders does not clip the
 * overflow — it lays the next row on top of it.
 *
 * Two things change it. A search adds a second line to rows that matched
 * on what they connect to, and the text scale grows the lines but not the
 * padding around them — so only the text portion is interpolated, which
 * is exact rather than merely closer than a constant. The same reasoning
 * as `editorRowHeight`, and for the same reason: the error repeats once
 * per row, and this list runs to tens of thousands of them.
 *
 * One height for the whole list rather than per row: the second line is
 * present or absent for every row at once, so measuring each would buy
 * nothing and cost the fixed-size fast path on a 46k-element network.
 * Rows with no second line centre in the taller slot.
 */
export function networkListRowHeight(
  scale: number,
  searching: boolean,
): number {
  const text = ID_LINE_AT_SCALE_1 + (searching ? CONTEXT_LINE_AT_SCALE_1 : 0);
  return Math.round(ROW_CHROME + text * scale);
}

/** Opacity of a row whose element is off screen. Low enough to recede at a
 * glance, high enough that the id stays readable — the row is still a
 * result, it is just not where you are looking. */
const OFFSCREEN_OPACITY = 0.38;

const DIM_PREF_KEY = "hydra2-rail-dim-offscreen";

function hasNodeCoordinates(node: Node): boolean {
  return !(node.x === 0 && node.y === 0);
}

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(id);
  }, [delayMs, value]);
  return debounced;
}

/** The measured row is inert; it exists to be sized, not used. */
const NOOP = () => {};

/** A row with nothing in it, for building the one the measurer renders. */
const BLANK_ROW: Row = {
  id: "",
  kind: "",
  cls: "point",
  context: "",
  value: null,
  format: null,
  canZoom: false,
};

/** What the value column's header says, and how wide a unit lane the rows
 * need beneath it. */
export interface ValueColumnHeading {
  /** Header text — a variable name, or the symbols when several are shown. */
  text: string;
  /** Tooltip spelling out the symbols; absent when the header is already
   * spelled out. */
  tip: string | undefined;
  /** Whether each row must carry its own unit. */
  perRowUnits: boolean;
  /** Width to reserve for that unit, in characters; 0 when unused. */
  unitWidth: number;
}

/**
 * The value column's header and unit lane, from the rows on screen.
 *
 * One slot, but not one meaning: with junctions and conduits side by side
 * it holds depth for some rows and flow for others. Naming the variables
 * is the only honest header — "Current" said nothing, and a column of bare
 * numbers meaning different things is worse than none.
 *
 * With a single variable the unit belongs in the header, and repeating it
 * on every row would only break the column's alignment. With several, each
 * row must carry its own — so the rows get a lane wide enough for the
 * widest, and the numbers right-align against one edge instead of being
 * shoved about by units of different widths.
 *
 * The lane is sized to what is displayed, not to the widest unit either
 * engine can produce: that is `ft/kft`, and a 320px rail cannot spare six
 * characters permanently for a variable rarely in view.
 *
 * Extracted and tested because the unit scan shares its loop — and its
 * early exit — with the symbol scan. That exit is correct only while a
 * class has exactly one variable, and nothing in the loop says so.
 */
export function valueColumnHeading(
  visible: readonly Row[],
  sys: UnitSystem,
): ValueColumnHeading {
  const symbolByName = new Map<string, string>();
  let unit = "";
  let unitWidth = 0;
  for (const r of visible) {
    const meta = formatMeta(r.format);
    if (!meta) continue;
    if (!symbolByName.has(meta.name)) {
      symbolByName.set(meta.name, meta.symbol);
      unitWidth = Math.max(unitWidth, unitOf(r, sys).length);
      if (symbolByName.size === 1) unit = unitOf(r, sys);
    }
    // One variable per class, so three is every variable there can be.
    if (symbolByName.size >= 3) break;
  }
  if (symbolByName.size === 0) {
    return { text: "", tip: undefined, perRowUnits: false, unitWidth: 0 };
  }
  if (symbolByName.size === 1) {
    const name = [...symbolByName.keys()][0];
    return {
      text: unit ? `${name} (${unit})` : name,
      tip: undefined,
      perRowUnits: false,
      unitWidth: 0,
    };
  }
  return {
    text: [...symbolByName.values()].join(" · "),
    tip: [...symbolByName.keys()].join(" · "),
    perRowUnits: true,
    unitWidth,
  };
}

/**
 * Whether this click should toggle the selection.
 *
 * `detail` is the browser's count of clicks in the current burst. Only the
 * first acts: selection *toggles*, so letting the second through undid the
 * first, and a double-click selected, deselected, then zoomed to something
 * it had just deselected.
 *
 * The alternative — waiting to see whether a second click arrives — puts a
 * quarter-second of latency on every selection to serve the rarer gesture.
 * The click count is free and already there.
 */
export function clickSelects(detail: number): boolean {
  return detail <= 1;
}

/**
 * Whether the double-click must select, having seen one click already.
 *
 * That click toggled, so a row that started selected is now deselected and
 * vice versa. Selecting whatever is not selected lands both starting
 * states on the same result — selected, and zoomed to — which is what
 * "double-click to zoom" should mean either way.
 */
export function doubleClickSelects(isActive: boolean): boolean {
  return !isActive;
}

/** Rank a row against a lowercased query. Lower is better; -1 is no match.
 *
 * The order is the order someone typing expects: what they typed exactly,
 * then things that begin with it, then things that contain it, and only
 * then things that merely mention it somewhere else. Without this, typing
 * "J-4" buries J-4 itself under J-40 through J-499. */
export function rankRow(row: Row, q: string): number {
  const id = row.id.toLowerCase();
  if (id === q) return 0;
  if (id.startsWith(q)) return 1;
  if (id.includes(q)) return 2;
  if (row.kind.toLowerCase().includes(q)) return 3;
  if (row.context.toLowerCase().includes(q)) return 4;
  return -1;
}

/** The value a row shows, and the column it came from.
 *
 * That column is the one the canvas legend has selected, so the number
 * beside an element is the number the map is painting it with — change the
 * legend and the list follows. Engines that serve a variable catalog put
 * their values in `resultValues`; an engine with fixed variables carries
 * them as fields on the element itself, so both are tried.
 */
function currentValue(
  el: Node | Link | Region,
  columns: SimResultColumn[] | undefined,
): Pick<Row, "value" | "format"> {
  const column = columns?.[0];
  if (!column) return { value: null, format: null };
  const bag = (el as { resultValues?: Record<string, number | null> })
    .resultValues;
  const raw =
    bag && column.key in bag
      ? bag[column.key]
      : (el as unknown as Record<string, unknown>)[column.key];
  const value = typeof raw === "number" && Number.isFinite(raw) ? raw : null;
  return { value, format: column };
}

// ── Styles ────────────────────────────────────────────────────────────────────

const HEADER_BAR: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 6,
  padding: "8px 10px 6px",
  borderBottom: "1px solid var(--border)",
  flexShrink: 0,
};

const SEARCH_INPUT: React.CSSProperties = {
  flex: 1,
  minWidth: 0,
  background: "var(--bg-input)",
  border: "1px solid var(--border)",
  borderRadius: 4,
  padding: "4px 8px",
  fontSize: "var(--text-sm)",
  color: "var(--text-primary)",
  outline: "none",
};

const CHIP_BASE: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
  padding: "2px 6px",
  borderRadius: 4,
  border: "1px solid transparent",
  background: "transparent",
  cursor: "pointer",
  fontSize: "var(--text-2xs)",
  color: "var(--text-tertiary)",
  fontFamily: "var(--font-ui)",
  flexShrink: 0,
};

const COUNT_BAR: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 6,
  padding: "3px 10px",
  fontSize: "var(--text-2xs)",
  letterSpacing: "0.05em",
  textTransform: "uppercase",
  color: "var(--text-tertiary)",
  borderBottom: "1px solid var(--border)",
  flexShrink: 0,
  whiteSpace: "nowrap",
};

// ── Panel ─────────────────────────────────────────────────────────────────────

interface Props {
  /** When omitted the close button is hidden (e.g. inside the rail). */
  onClose?: () => void;
  onSelectNode: (id: string) => void;
  onSelectLink: (id: string) => void;
  /** Override the internal `useNodes()` call (e.g. merged sim-result nodes). */
  nodes?: Node[];
  links?: Link[];
  regions?: Region[];
  activeNodeId?: string | null;
  activeLinkId?: string | null;
  onZoomToNode?: (id: string) => void;
  onZoomToLink?: (id: string) => void;
  onSelectRegion?: (id: string) => void;
  onZoomToRegion?: (id: string) => void;
  /** Registers a function reporting the width that would fit every listed
   *  row, for the rail's fit-to-content gesture. */
  onFitWidth?: (measure: () => number | null) => void;
  activeRegionId?: string | null;
  /** Generic result-column headers (engines whose values ride on
   * `resultValues`); absent for wds, whose pressure and flow ride inline. */
  nodeResultColumns?: SimResultColumn[];
  linkResultColumns?: SimResultColumn[];
  regionResultColumns?: SimResultColumn[];
  /** Render inline (fills its container) rather than as an overlay. */
  embedded?: boolean;
}

export function NetworkList({
  onClose,
  onSelectNode,
  onSelectLink,
  nodes: nodesProp,
  links: linksProp,
  regions: regionsProp,
  activeNodeId,
  activeLinkId,
  onZoomToNode,
  onZoomToLink,
  onSelectRegion,
  onZoomToRegion,
  onFitWidth,
  activeRegionId,
  nodeResultColumns,
  linkResultColumns,
  regionResultColumns,
  embedded,
}: Props) {
  const sys = useUnitSystem();
  // Setters only — this panel never reads hover state, so moving the pointer
  // down a list of 46k rows never re-renders it.
  const { hoverNode, hoverLink, hoverRegion, clearHover } = useHoverActions();
  const internalNodes = useNodes();
  const internalLinks = useLinks();
  const internalRegions = useRegions();
  const allNodes = nodesProp ?? internalNodes;
  const allLinks = linksProp ?? internalLinks;
  const regions = regionsProp ?? internalRegions;

  const { engine } = useActiveProject();
  const elementKinds = useElementKinds(engine?.key);
  const kindLabel = useMemo(() => {
    const m = new Map<string, string>();
    for (const k of elementKinds) m.set(k.id, k.label);
    return m;
  }, [elementKinds]);

  // Derived from the base network, not the sim-merged arrays: those change
  // identity on every timeline scrub, but coordinates never do.
  const zoomableNodeIds = useMemo(
    () => new Set(internalNodes.filter(hasNodeCoordinates).map((n) => n.id)),
    [internalNodes],
  );

  const [queryInput, setQueryInput] = useState("");
  const query = useDebouncedValue(queryInput, 120);
  const [kindFilter, setKindFilter] = useState<string | null>(null);

  // `null` means the canvas has no geographic viewport (schematic, local
  // grid) — the toggle is hidden rather than offered and inert. Reading the
  // key here is also what re-renders the rows as the map moves.
  const viewportKey = useViewportKey();
  const { isInViewport } = useViewportActions();
  const [dimOffscreen, setDimOffscreen] = useState(
    () => localStorage.getItem(DIM_PREF_KEY) === "true",
  );
  function toggleDim() {
    setDimOffscreen((on) => {
      localStorage.setItem(DIM_PREF_KEY, String(!on));
      return !on;
    });
  }
  const dimming = dimOffscreen && viewportKey != null;

  // One row per element, every class in one sequence. Rebuilt whenever the
  // network or its current-period values change — the same cost the three
  // tabs paid, now paid once.
  const rows = useMemo<Row[]>(() => {
    const t0 = performance.now();
    const out: Row[] = [];
    for (const n of allNodes) {
      out.push({
        id: n.id,
        kind: n.type,
        cls: "point",
        context: "",
        canZoom: zoomableNodeIds.has(n.id),
        ...currentValue(n, nodeResultColumns),
      });
    }
    for (const l of allLinks) {
      out.push({
        id: l.id,
        kind: l.type,
        cls: "polyline",
        context: `${l.fromId} → ${l.toId}`,
        canZoom: zoomableNodeIds.has(l.fromId) && zoomableNodeIds.has(l.toId),
        ...currentValue(l, linkResultColumns),
      });
    }
    for (const r of regions) {
      out.push({
        id: r.id,
        kind: r.type,
        cls: "region",
        context: r.outletId ? `→ ${r.outletId}` : "",
        canZoom: true,
        ...currentValue(r, regionResultColumns),
      });
    }
    perfTrace("network-finder-rows", performance.now() - t0, {
      count: out.length,
    });
    return out;
  }, [
    allNodes,
    allLinks,
    regions,
    zoomableNodeIds,
    nodeResultColumns,
    linkResultColumns,
    regionResultColumns,
  ]);

  /** Kinds actually present, in catalog order, with their counts — the chip
   * strip. A kind the model does not contain earns no chip. */
  const presentKinds = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of rows) counts.set(r.kind, (counts.get(r.kind) ?? 0) + 1);
    const ordered = elementKinds
      .filter((k) => counts.has(k.id))
      .map((k) => ({
        id: k.id,
        label: k.labelPlural,
        count: counts.get(k.id) ?? 0,
      }));
    // Kinds the catalog does not declare still deserve a chip rather than
    // becoming unreachable behind a filter that cannot name them.
    for (const [id, count] of counts) {
      if (!ordered.some((k) => k.id === id)) {
        ordered.push({ id, label: id, count });
      }
    }
    return ordered;
  }, [rows, elementKinds]);

  // A filter for a kind the model no longer has would hide everything with
  // no way back, so it lapses with the model.
  const activeKind =
    kindFilter && presentKinds.some((k) => k.id === kindFilter)
      ? kindFilter
      : null;

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const byKind = activeKind
      ? rows.filter((r) => r.kind === activeKind)
      : rows;
    if (!q) return byKind;
    const t0 = performance.now();
    const ranked: Array<{ row: Row; rank: number; i: number }> = [];
    byKind.forEach((row, i) => {
      const rank = rankRow(row, q);
      if (rank >= 0) ranked.push({ row, rank, i });
    });
    // Stable within a rank: equal-quality matches keep model order rather
    // than shuffling as you type.
    ranked.sort((a, b) => a.rank - b.rank || a.i - b.i);
    perfTrace("network-finder-search", performance.now() - t0, {
      matched: ranked.length,
    });
    return ranked.map((r) => r.row);
  }, [rows, query, activeKind]);

  const searching = query.trim().length > 0;

  // The widest content the list currently holds, rendered once off-screen
  // so the panel can be fitted to it. A synthetic row rather than a real
  // one: the longest id and the longest subtitle need not belong to the
  // same element, and what has to fit is the widest of each.
  const fit = useMemo(
    () => fitContent(visible, searching),
    [visible, searching],
  );
  const measureRow: Row | null = useMemo(() => {
    if (!fit) return null;
    const column = linkResultColumns?.[0] ?? nodeResultColumns?.[0] ?? null;
    // Whichever extreme renders longer sets the value lane — two
    // formatting calls for the whole list, not one per row.
    const value =
      fit.extremes == null
        ? null
        : formatValue(
              { ...BLANK_ROW, value: fit.extremes[0], format: column },
              sys,
            ).length >=
            formatValue(
              { ...BLANK_ROW, value: fit.extremes[1], format: column },
              sys,
            ).length
          ? fit.extremes[0]
          : fit.extremes[1];
    return {
      ...BLANK_ROW,
      id: fit.id,
      context: fit.context ?? "",
      canZoom: fit.zoomable,
      value,
      format: column,
    };
  }, [fit, linkResultColumns, nodeResultColumns, sys]);

  const valueHeading = useMemo(
    () => valueColumnHeading(visible, sys),
    [visible, sys],
  );

  const scrollRef = useRef<HTMLDivElement>(null);
  const measureRef = useRef<HTMLDivElement>(null);

  // Report the width this list would need to show everything, so the rail
  // can fit itself to it. Measured off a rendered row rather than
  // computed, so the badge lane, gaps and padding are the row's own.
  useEffect(() => {
    if (!onFitWidth) return;
    onFitWidth(() => {
      const row = measureRef.current?.firstElementChild;
      const scroller = scrollRef.current;
      if (!row || !scroller) return null;
      // The scrollbar is inside the panel, so its track is width the rows
      // do not get. Zero where scrollbars are overlays.
      const scrollbar = scroller.offsetWidth - scroller.clientWidth;
      return Math.ceil(row.getBoundingClientRect().width + scrollbar);
    });
  }, [onFitWidth]);
  const rowHeight = networkListRowHeight(readTextScale(), searching);
  const rowVirtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });

  // The virtualiser caches measurements, so a changed `estimateSize` alone
  // leaves every row positioned at the old pitch — the first search would
  // lay the taller rows out on the shorter spacing.
  // biome-ignore lint/correctness/useExhaustiveDependencies: remeasure on pitch change
  useEffect(() => {
    rowVirtualizer.measure();
  }, [rowHeight, rowVirtualizer]);

  // Class *and* id: an element id is unique only within its class, so a
  // junction and a pipe may both be called "2".
  const active = activeElement(activeNodeId, activeLinkId, activeRegionId);
  const activeScrollKey = activeKey(active);

  // Selection arriving from the canvas scrolls its row into view. Keyed on
  // the selected element rather than the whole list: re-running on every
  // list identity would fight the user's own scrolling on each timeline
  // scrub.
  const lastScrolledTo = useRef<string | null>(null);
  useEffect(() => {
    if (active == null || activeScrollKey == null) {
      lastScrolledTo.current = null;
      return;
    }
    if (lastScrolledTo.current === activeScrollKey) return;
    const index = visible.findIndex((r) => isActiveRow(r, active));
    if (index < 0) return;
    lastScrolledTo.current = activeScrollKey;
    rowVirtualizer.scrollToIndex(index, { align: "auto" });
  }, [active, activeScrollKey, visible, rowVirtualizer]);

  /** Hovering a row lights the element up on the canvas, exactly as
   * hovering the element itself does. */
  function hover(row: Row) {
    if (row.cls === "point") hoverNode(row.id);
    else if (row.cls === "polyline") hoverLink(row.id);
    else hoverRegion(row.id);
  }

  function select(row: Row) {
    if (row.cls === "point") onSelectNode(row.id);
    else if (row.cls === "polyline") onSelectLink(row.id);
    else onSelectRegion?.(row.id);
  }

  function zoomTo(row: Row) {
    if (row.cls === "point") onZoomToNode?.(row.id);
    else if (row.cls === "polyline") onZoomToLink?.(row.id);
    else onZoomToRegion?.(row.id);
  }

  function canZoomTo(row: Row): boolean {
    if (!row.canZoom) return false;
    if (row.cls === "point") return onZoomToNode != null;
    if (row.cls === "polyline") return onZoomToLink != null;
    return onZoomToRegion != null;
  }

  const shell: React.CSSProperties = embedded
    ? {
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      }
    : {
        position: "absolute",
        right: 0,
        top: 0,
        bottom: 0,
        width: 320,
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      };

  return (
    <div className="side-panel" style={shell}>
      <div style={HEADER_BAR}>
        <input
          value={queryInput}
          onChange={(e) => setQueryInput(e.target.value)}
          placeholder="Find an element…"
          aria-label="Find a network element"
          spellCheck={false}
          autoComplete="off"
          style={SEARCH_INPUT}
        />
        {queryInput && (
          <button
            type="button"
            onClick={() => setQueryInput("")}
            aria-label="Clear search"
            data-tooltip="Clear search"
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              color: "var(--text-tertiary)",
              display: "flex",
              padding: 2,
            }}
          >
            <XMarkIcon width={13} height={13} />
          </button>
        )}
        {viewportKey != null && (
          <button
            type="button"
            onClick={toggleDim}
            aria-label="Dim elements outside the map view"
            aria-pressed={dimOffscreen}
            data-tooltip={
              dimOffscreen
                ? "Showing all — elements off the map view are dimmed"
                : "Dim elements outside the map view"
            }
            style={{
              background: dimOffscreen
                ? "var(--selection-bg-strong)"
                : "transparent",
              border: `1px solid ${
                dimOffscreen ? "var(--selection-border)" : "transparent"
              }`,
              borderRadius: 4,
              cursor: "pointer",
              color: dimOffscreen ? "var(--accent)" : "var(--text-tertiary)",
              display: "flex",
              padding: 3,
              flexShrink: 0,
            }}
          >
            <EyeIcon width={13} height={13} />
          </button>
        )}
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              color: "var(--text-tertiary)",
              display: "flex",
              padding: 2,
            }}
          >
            <XMarkIcon width={14} height={14} />
          </button>
        )}
      </div>

      {presentKinds.length > 1 && (
        <div
          style={{
            display: "flex",
            gap: 3,
            padding: "6px 8px",
            overflowX: "auto",
            scrollbarWidth: "none",
            flexShrink: 0,
            borderBottom: "1px solid var(--border)",
          }}
        >
          <button
            type="button"
            onClick={() => setKindFilter(null)}
            data-tooltip="Every kind"
            style={{
              ...CHIP_BASE,
              color: activeKind == null ? "var(--accent)" : CHIP_BASE.color,
              background:
                activeKind == null
                  ? "var(--selection-bg-strong)"
                  : "transparent",
              borderColor:
                activeKind == null ? "var(--selection-border)" : "transparent",
            }}
          >
            All
          </button>
          {presentKinds.map((k) => {
            const on = activeKind === k.id;
            return (
              <button
                type="button"
                key={k.id}
                onClick={() => setKindFilter(on ? null : k.id)}
                data-tooltip={`${k.label} (${k.count})`}
                style={{
                  ...CHIP_BASE,
                  background: on ? "var(--selection-bg-strong)" : "transparent",
                  borderColor: on ? "var(--selection-border)" : "transparent",
                }}
              >
                <TypeBadge type={k.id} size="sm" />
                <span>{k.count}</span>
              </button>
            );
          })}
        </div>
      )}

      <div style={COUNT_BAR}>
        <span>
          {searching || activeKind
            ? `${visible.length.toLocaleString()} of ${rows.length.toLocaleString()}`
            : `${rows.length.toLocaleString()} elements`}
        </span>
        <span
          data-tooltip={valueHeading.tip}
          style={{
            overflow: "hidden",
            textOverflow: "ellipsis",
            textTransform: "none",
            letterSpacing: 0,
          }}
        >
          {valueHeading.text}
        </span>
      </div>

      {visible.length === 0 ? (
        <div
          style={{
            padding: 14,
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            fontStyle: "italic",
          }}
        >
          {rows.length === 0
            ? "This project has no network yet."
            : "Nothing matches."}
        </div>
      ) : (
        <div
          ref={scrollRef}
          style={{ flex: 1, overflow: "auto", minHeight: 0 }}
        >
          <div
            style={{
              height: rowVirtualizer.getTotalSize(),
              position: "relative",
            }}
          >
            {rowVirtualizer.getVirtualItems().map((v) => {
              const row = visible[v.index];
              const isActive = isActiveRow(row, active);
              const zoomable = canZoomTo(row);
              // Probed per rendered row — the virtualizer keeps that to a
              // couple of dozen. The selected row never dims: having panned
              // away from it is exactly when you still need to see it.
              const offscreen =
                dimming && !isActive && !isInViewport(row.cls, row.id);
              return (
                <div
                  key={`${row.cls}:${row.id}`}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: v.size,
                    transform: `translateY(${v.start}px)`,
                    opacity: offscreen ? OFFSCREEN_OPACITY : 1,
                  }}
                >
                  <NetworkListRow
                    row={row}
                    isActive={isActive}
                    zoomable={zoomable}
                    searching={searching}
                    sys={sys}
                    valueHeading={valueHeading}
                    kindLabel={kindLabel}
                    onSelect={select}
                    onZoom={zoomTo}
                    onHover={hover}
                    onClearHover={clearHover}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* The row the panel would have to be wide enough to show, kept off
          screen. Rendered rather than modelled: the width that matters is
          the one this component produces, and a second description of its
          badge lane, gaps and padding would drift from it.

          One row, so the virtualiser is beside the point — and it changes
          only when the listed content does. */}
      {measureRow && (
        <div
          ref={measureRef}
          aria-hidden
          style={{
            position: "absolute",
            visibility: "hidden",
            pointerEvents: "none",
            top: 0,
            left: 0,
            width: "max-content",
          }}
        >
          <NetworkListRow
            intrinsic
            row={measureRow}
            isActive={false}
            zoomable={measureRow.canZoom}
            searching={searching}
            sys={sys}
            valueHeading={valueHeading}
            kindLabel={kindLabel}
            onSelect={NOOP}
            onZoom={NOOP}
            onHover={NOOP}
            onClearHover={NOOP}
          />
        </div>
      )}
    </div>
  );
}
