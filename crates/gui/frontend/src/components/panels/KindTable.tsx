// ── The per-kind element table ────────────────────────────────────────────────
//
// One kind, one table, all of its own columns — the arrangement a shared
// Nodes/Links table cannot offer, because columns common to junctions,
// outfalls and storage units are barely any columns at all.
//
// Properties only: what the model file declares. Results are deliberately
// absent. They belong to a moment in a run, and the page this table lives on
// has no timeline to choose that moment with — a column headed "current"
// beside a scrub bar that does not exist answers a question nobody asked.
// Results live where the timeline does: the canvas rail and the element
// inspector.
//
// Columns are engine-authored — labels, units and ordering come from the
// engine's §4.4 attribute schema — so a kind this file has never heard of
// renders correctly, and so does an engine that does not exist yet.

import {
  ChevronDownIcon,
  ChevronUpDownIcon,
  ChevronUpIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import type { KindElements } from "../../hooks";
import { formatElementAttribute } from "../../hooks/network";
import { useUnitSystem } from "../../units";

type SortDir = "asc" | "desc";

/**
 * The sort mark beside a column heading.
 *
 * Sized in `em` rather than pixels because it sits inside the heading's
 * own text: the app's text-size setting moves that text through five
 * steps, and an icon pinned to one of them drifts from the word it
 * belongs to at the other four. The baseline nudge centres it against
 * lowercase rather than letting it sit on the baseline.
 */
const SORT_ICON: React.CSSProperties = {
  width: "1em",
  height: "1em",
  marginLeft: 3,
  verticalAlign: "-0.15em",
  flexShrink: 0,
};

const TH: React.CSSProperties = {
  padding: "6px 10px",
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

const TD: React.CSSProperties = {
  padding: "5px 10px",
  fontSize: "var(--text-md)",
  borderBottom: "1px solid rgba(255,255,255,0.04)",
  whiteSpace: "nowrap",
};

/**
 * One element kind's table.
 *
 * Carries no kind column: the table is already scoped to a single kind by
 * the Editor's rail, so a badge on every row would repeat the same glyph
 * down the page and buy a column's width for nothing.
 *
 * Mount one per kind (`key={kind}`) — the sort column belongs to the kind
 * being shown, and carrying it across would leave a table sorted by a
 * column it does not have.
 */
export function KindTable({
  elements,
  activeId,
  onSelect,
  revealToken,
}: {
  /** §4.4 property columns for this kind. */
  elements: KindElements;
  activeId?: string | null;
  onSelect?: (id: string) => void;
  /**
   * Bumped by the caller to mean "bring `activeId` into view now".
   *
   * A token rather than a boolean because the same element can be revealed
   * twice in a row — asking for J5 again after scrolling away has to move
   * the table again, and a boolean that is already `true` cannot say so.
   */
  revealToken?: number;
}) {
  const sys = useUnitSystem();
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [query, setQuery] = useState("");
  const activeRowRef = useRef<HTMLTableRowElement | null>(null);

  // Reveal: clear any search first, because a filter that excludes the
  // requested element would leave the table looking empty in response to
  // "show me this element" — then scroll once the row has rendered.
  useEffect(() => {
    if (revealToken == null) return;
    setQuery("");
    const raf = requestAnimationFrame(() => {
      activeRowRef.current?.scrollIntoView({ block: "center" });
    });
    return () => cancelAnimationFrame(raf);
  }, [revealToken]);

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

  // Matching is on the id alone, not every column.
  //
  // A drainage model has thousands of conduits, and the question a search
  // box answers here is "where is C1423?" — searching the property values
  // as well would return rows whose diameter happens to contain the digits
  // typed, burying the one that was asked for.
  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return null;
    return new Set(
      elements.ids
        .map((id, i) => (id.toLowerCase().includes(q) ? i : -1))
        .filter((i) => i >= 0),
    );
  }, [elements.ids, query]);

  // Rows are indices into the columnar arrays, so sorting never copies the
  // values themselves.
  const order = useMemo(() => {
    const idx = elements.ids
      .map((_, i) => i)
      .filter((i) => matches == null || matches.has(i));
    if (!sortCol) return idx;
    const propCol = elements.columns.find((c) => c.key === sortCol);
    const get = (i: number): number | string | null => {
      if (propCol) return propCol.values[i] ?? null;
      return elements.ids[i];
    };
    return idx.sort((a, b) => {
      const av = get(a) ?? "";
      const bv = get(b) ?? "";
      const cmp = av < bv ? -1 : av > bv ? 1 : 0;
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [elements, sortCol, sortDir, matches]);

  const indicator = (col: string) => {
    if (sortCol !== col) {
      return <ChevronUpDownIcon style={{ ...SORT_ICON, opacity: 0.25 }} />;
    }
    const Arrow = sortDir === "asc" ? ChevronUpIcon : ChevronDownIcon;
    return <Arrow style={{ ...SORT_ICON, color: "var(--accent)" }} />;
  };

  if (elements.ids.length === 0) {
    return (
      <div
        style={{
          padding: 16,
          fontSize: "var(--text-md)",
          color: "var(--text-tertiary)",
        }}
      >
        No elements of this kind.
      </div>
    );
  }

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "flex-end",
          padding: "8px 12px",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-panel)",
          flexShrink: 0,
        }}
      >
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search ids…"
          aria-label="Search ids"
          style={{
            width: 200,
            height: 28,
            background: "var(--bg-input)",
            border: "1px solid var(--border)",
            borderRadius: 5,
            padding: "0 8px",
            color: "var(--text-primary)",
            fontFamily: "var(--font-mono)",
            fontSize: "var(--text-md)",
            outline: "none",
          }}
        />
      </div>
      {/* A search that matches nothing is not the same as a kind with
          nothing in it, and saying "no elements of this kind" here would
          be false. */}
      {order.length === 0 ? (
        <div
          style={{
            padding: 16,
            fontSize: "var(--text-md)",
            color: "var(--text-tertiary)",
          }}
        >
          No ids match “{query}”.
        </div>
      ) : (
        <div style={{ overflow: "auto", flex: 1 }}>
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              WebkitUserSelect: "none",
              userSelect: "none",
            }}
          >
            <thead>
              <tr>
                <th style={TH} onClick={() => toggleSort("id")}>
                  ID{indicator("id")}
                </th>
                {elements.columns.map((c) => (
                  <th key={c.key} style={TH} onClick={() => toggleSort(c.key)}>
                    {c.label}
                    {c.quantity
                      ? ` (${sys === "us" ? c.quantity.usLabel : c.quantity.siLabel})`
                      : ""}
                    {indicator(c.key)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {order.map((i) => {
                const id = elements.ids[i];
                const isActive = id === activeId;
                return (
                  <tr
                    key={id}
                    ref={isActive ? activeRowRef : undefined}
                    onClick={() => onSelect?.(id)}
                    style={{
                      cursor: onSelect ? "pointer" : undefined,
                      background: isActive ? "var(--selection-bg)" : undefined,
                      outline: isActive
                        ? "1px solid var(--selection-border)"
                        : undefined,
                      outlineOffset: "-1px",
                    }}
                  >
                    <td
                      style={{
                        ...TD,
                        color: "var(--accent)",
                        fontFamily: "var(--font-mono)",
                        fontWeight: 500,
                      }}
                    >
                      {id}
                    </td>
                    {elements.columns.map((c) => {
                      const v = c.values[i];
                      return (
                        <td
                          key={c.key}
                          style={{ ...TD, fontFamily: "var(--font-mono)" }}
                        >
                          {v == null
                            ? "—"
                            : typeof v === "number"
                              ? formatElementAttribute(
                                  {
                                    label: c.label,
                                    number: v,
                                    quantity: c.quantity,
                                  },
                                  sys,
                                )
                              : v}
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
