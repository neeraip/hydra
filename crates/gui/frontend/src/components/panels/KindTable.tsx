// ── The per-kind element table ────────────────────────────────────────────────
//
// One kind, one table, all of its own columns — the arrangement a shared
// Nodes/Links table cannot offer, because columns common to junctions,
// outfalls and storage units are barely any columns at all.
//
// The vertical divider down the middle is the point of the design, and it
// separates two different kinds of truth:
//
//   left   §4.3 properties — what the model file declares. Fixed for a run.
//   right  §6 results      — what the simulation produced, at the period
//                            the timeline is parked on. Changes as you scrub.
//
// Both sides are engine-authored: labels, units and ordering come from the
// engine's catalogs, so a kind this file has never heard of renders
// correctly, and so does an engine that does not exist yet.

import { useMemo, useState } from "react";
import type { GenericVariable, KindElements } from "../../hooks";
import { formatGenericValue, genericUnitLabel } from "../../hooks";
import { formatElementAttribute } from "../../hooks/network";
import { useUnitSystem } from "../../units";
import { TypeBadge } from "../ui/TypeBadge";

type SortDir = "asc" | "desc";

/** Per-element current-period values, keyed by variable id — the same bag
 * the rail and inspector read, indexed here by element id. */
export type ResultValuesById = Map<string, Record<string, number | null>>;

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

/** The rule that draws the divider: the first results column carries it, so
 * the boundary sits between the two families without an empty spacer
 * column that would confuse selection and export. */
const DIVIDER: React.CSSProperties = {
  borderLeft: "2px solid var(--border-hover)",
};

export function KindTable({
  kindId,
  elements,
  resultVariables,
  resultValues,
  activeId,
  onSelect,
}: {
  kindId: string;
  /** §4.3 property columns for this kind. */
  elements: KindElements;
  /** §6 result variables for this kind's class; empty before a run. */
  resultVariables: GenericVariable[];
  /** Current-period values per element id; empty before a run. */
  resultValues: ResultValuesById;
  activeId?: string | null;
  onSelect?: (id: string) => void;
}) {
  const sys = useUnitSystem();
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("asc");

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

  // Rows are indices into the columnar arrays, so sorting never copies the
  // values themselves.
  const order = useMemo(() => {
    const idx = elements.ids.map((_, i) => i);
    if (!sortCol) return idx;
    const propCol = elements.columns.find((c) => c.key === sortCol);
    const get = (i: number): number | string | null => {
      if (propCol) return propCol.values[i] ?? null;
      if (sortCol === "id") return elements.ids[i];
      return resultValues.get(elements.ids[i])?.[sortCol] ?? null;
    };
    return idx.sort((a, b) => {
      const av = get(a) ?? "";
      const bv = get(b) ?? "";
      const cmp = av < bv ? -1 : av > bv ? 1 : 0;
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [elements, sortCol, sortDir, resultValues]);

  const indicator = (col: string) =>
    sortCol !== col ? (
      <span style={{ opacity: 0.25, marginLeft: 3 }}>↕</span>
    ) : (
      <span style={{ marginLeft: 3, color: "var(--accent)" }}>
        {sortDir === "asc" ? "↑" : "↓"}
      </span>
    );

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
    <div style={{ overflow: "auto", flex: 1 }}>
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          userSelect: "none",
        }}
      >
        <thead>
          <tr>
            <th style={{ ...TH, width: 34 }} />
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
            {resultVariables.map((v, i) => {
              const unit = genericUnitLabel(v.quantity, sys);
              return (
                <th
                  key={v.id}
                  style={i === 0 ? { ...TH, ...DIVIDER } : TH}
                  onClick={() => toggleSort(v.id)}
                  data-tooltip="Result at the current timestep"
                >
                  {v.label}
                  {unit ? ` (${unit})` : ""}
                  {indicator(v.id)}
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {order.map((i) => {
            const id = elements.ids[i];
            const isActive = id === activeId;
            const values = resultValues.get(id);
            return (
              <tr
                key={id}
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
                <td style={{ ...TD, padding: "5px 4px", textAlign: "center" }}>
                  <TypeBadge type={kindId} />
                </td>
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
                {resultVariables.map((variable, ri) => (
                  <td
                    key={variable.id}
                    style={
                      ri === 0
                        ? {
                            ...TD,
                            ...DIVIDER,
                            fontFamily: "var(--font-mono)",
                          }
                        : { ...TD, fontFamily: "var(--font-mono)" }
                    }
                  >
                    {formatGenericValue(
                      values?.[variable.id],
                      variable.quantity,
                      sys,
                      false,
                    )}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
