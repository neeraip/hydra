import { MagnifyingGlassPlusIcon } from "@heroicons/react/16/solid";
import { categoryRgba } from "../../canvas/MapCanvas/colorUtils";
import type { SimResultColumn } from "../../canvas/selection-context";
import type { ElementClass } from "../../hooks";
import { formatGenericValue, genericUnitLabel } from "../../hooks";
import { toDisplay, type UnitSystem, unitLabel } from "../../units";
import { MiddleTruncate } from "../ui/MiddleTruncate";
import { TypeBadge } from "../ui/TypeBadge";
import type { ValueColumnHeading } from "./NetworkList";
import { clickSelects, doubleClickSelects } from "./NetworkList";

// ── The row model ─────────────────────────────────────────────────────────────

/** One line of the finder. Classes are flattened into a single sequence, so
 * everything the list needs to draw and rank a row lives here rather than
 * behind three parallel code paths. */
export interface Row {
  id: string;
  /** Element-kind id — drives the badge and the kind chips. */
  kind: string;
  cls: ElementClass;
  /** Secondary text the search also matches: what a link joins, or the node
   * a catchment drains to. Shown as the row's subtitle when searching. */
  context: string;
  /** Current-period value, already SI, or null before a run. */
  value: number | null;
  /** The column this value came from, for its label and unit. */
  format: SimResultColumn | null;
  canZoom: boolean;
}

export function formatValue(row: Row, sys: "si" | "us"): string {
  if (row.value == null || !row.format) return "—";
  // A coded column brings its own labels, so the list never has to know
  // which variables are enumerations. An unrecognised code falls through
  // to the number, which is more use than a dash when the engine has
  // grown a state this build does not name.
  if (row.format.codes) {
    return row.format.codes[row.value]?.label ?? String(row.value);
  }
  if (row.format.quantity) {
    return formatGenericValue(row.value, row.format.quantity, sys, false);
  }
  if (row.format.unit) {
    return toDisplay(row.value, row.format.unit, sys).toFixed(
      sys === "si" ? 2 : 1,
    );
  }
  // Dimensionless: quality carries whatever unit its mode implies, and a
  // status code is an enum. Neither converts.
  return String(Number(row.value.toFixed(2)));
}

/**
 * The colour a row's value is drawn in.
 *
 * A coded value is coloured by the severity its engine gave the state —
 * the same judgement the legend and the canvas colour from, so a closed
 * link reads the same on all three. States the engine passed no judgement
 * on, and every measured value, keep the column's ordinary foreground: a
 * land-use class or a material has no alarming member, and inventing one
 * would be the app asserting something the engine declined to.
 */
export function valueColor(row: Row): string {
  if (row.value == null) return "var(--text-disabled)";
  const severity = row.format?.codes?.[row.value]?.severity;
  if (!severity) return "var(--text-secondary)";
  const [r, g, b] = categoryRgba(0, 255, severity);
  return `rgb(${r}, ${g}, ${b})`;
}

/** The name and engineering symbol for a row's value. */
export function formatMeta(
  f: Row["format"],
): { name: string; symbol: string } | null {
  if (f == null) return null;
  return {
    name: f.label,
    symbol: f.symbol ?? f.label.charAt(0).toUpperCase(),
  };
}

export function unitOf(row: Row, sys: "si" | "us"): string {
  if (!row.format) return "";
  if (row.format.quantity) {
    return genericUnitLabel(row.format.quantity, sys) ?? "";
  }
  return row.format.unit ? unitLabel(row.format.unit, sys) : "";
}

/** Width of the badge lane, which every row reserves whether or not its
 *  kind has a badge to put there. */
export const BADGE_COL_WIDTH = 28;

/**
 * One row of the finder.
 *
 * Its own component so that fitting the panel to its contents can render
 * a real row and measure it, rather than measuring a second copy of this
 * markup that would drift from it. That is the whole reason for the
 * extraction — the list still renders it the same way.
 */
export function NetworkListRow({
  row,
  isActive,
  zoomable,
  searching,
  sys,
  valueHeading,
  kindLabel,
  onSelect,
  onZoom,
  onHover,
  onClearHover,
  intrinsic = false,
}: {
  row: Row;
  isActive: boolean;
  zoomable: boolean;
  searching: boolean;
  sys: UnitSystem;
  valueHeading: ValueColumnHeading;
  kindLabel: ReadonlyMap<string, string>;
  onSelect: (row: Row) => void;
  onZoom: (row: Row) => void;
  onHover: (row: Row) => void;
  onClearHover: () => void;
  /**
   * Size to the row's content instead of filling its container.
   *
   * Set only when measuring: a row rendered to find the panel's ideal
   * width has to report what it *wants*, and a row that fills whatever it
   * is given reports the container back.
   */
  intrinsic?: boolean;
}) {
  const select = onSelect;
  const zoomTo = onZoom;
  const hover = onHover;
  const clearHover = onClearHover;
  return (
    <>
      {/* The row is the button; the zoom control is its sibling
          rather than its child, because a button inside a button
          is invalid and gives the row two focus stops. */}
      <button
        type="button"
        // Select on click, zoom on double-click — the file-list
        // idiom, where opening is what you do to the thing you
        // just picked.
        //
        // Neither handler waits to see whether another click is
        // coming. Debouncing the first would put a quarter-second
        // of latency on every selection to serve the rarer
        // gesture, so instead each one reads `event.detail`, the
        // browser's own count of clicks in this burst, and the
        // pair is arranged to land on the same result either way.
        onClick={(e) => {
          // Selection *toggles*, so letting the second click
          // through undid the first: a double-click selected,
          // deselected, then zoomed to something no longer
          // selected. The second click of a burst does nothing
          // and leaves the outcome to the double-click handler.
          if (!clickSelects(e.detail)) return;
          select(row);
        }}
        onDoubleClick={
          zoomable
            ? () => {
                // One click has landed, so a row that started
                // selected is now deselected and vice versa.
                // Select whatever is not, and a double-click
                // always ends selected *and* zoomed, from
                // either starting state.
                if (doubleClickSelects(isActive)) {
                  select(row);
                }
                zoomTo(row);
              }
            : undefined
        }
        style={{
          width: intrinsic ? "max-content" : "100%",
          height: "100%",
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: `0 ${zoomable ? 26 : 8}px 0 8px`,
          textAlign: "left",
          font: "inherit",
          cursor: "pointer",
          userSelect: "none",
          boxSizing: "border-box",
          border: "none",
          borderBottom: "1px solid rgba(255,255,255,0.04)",
          background: isActive ? "var(--selection-bg)" : "transparent",
          outline: isActive ? "1px solid var(--selection-border)" : undefined,
          outlineOffset: "-1px",
        }}
        onMouseEnter={(e) => {
          if (!isActive)
            e.currentTarget.style.background = "rgba(255,255,255,0.04)";
          hover(row);
        }}
        onMouseLeave={(e) => {
          if (!isActive) e.currentTarget.style.background = "transparent";
          clearHover();
        }}
        onFocus={() => hover(row)}
        onBlur={() => clearHover()}
      >
        <span
          style={{
            width: BADGE_COL_WIDTH - 6,
            display: "flex",
            justifyContent: "center",
            flexShrink: 0,
          }}
          data-tooltip={kindLabel.get(row.kind) ?? row.kind}
        >
          <TypeBadge type={row.kind} />
        </span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            ...(intrinsic
              ? { flexBasis: "max-content", minWidth: "auto" }
              : null),
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            overflow: "hidden",
          }}
        >
          <span
            style={{
              color: "var(--accent)",
              fontFamily: "var(--font-mono)",
              fontSize: "var(--text-sm)",
              fontWeight: 500,
              overflow: "hidden",
            }}
          >
            <MiddleTruncate text={row.id} />
          </span>
          {/* What a row connects to is the disambiguator when a
            query matched it there rather than on its id. */}
          {searching && row.context && (
            <span
              style={{
                fontSize: "var(--text-2xs)",
                color: "var(--text-tertiary)",
                fontFamily: "var(--font-mono)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {row.context}
            </span>
          )}
        </span>
        <span
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: "var(--text-sm)",
            color: valueColor(row),
            flexShrink: 0,
          }}
          data-tooltip={
            row.value != null
              ? `${formatMeta(row.format)?.name ?? ""} ${unitOf(
                  row,
                  sys,
                )}`.trim() || undefined
              : undefined
          }
        >
          {formatValue(row, sys)}
          {/* A lane of its own, so the numbers right-align on
              one edge instead of being pushed around by units
              of different widths — "8.153 m" and "0.8695 m/s"
              right-aligned as one group put their digits at
              different offsets, which is exactly what the
              column exists to let you compare.

              Reserved even when a row has no value, or the
              rows that do would shift out from under the ones
              that don't. */}
          {valueHeading.perRowUnits && (
            <span
              style={{
                color: "var(--text-tertiary)",
                marginLeft: 3,
                display: "inline-block",
                width: `${valueHeading.unitWidth}ch`,
                textAlign: "left",
              }}
            >
              {row.value != null ? unitOf(row, sys) : ""}
            </span>
          )}
        </span>
      </button>
      {zoomable && (
        <button
          type="button"
          onClick={() => zoomTo(row)}
          aria-label={`Zoom to ${row.id}`}
          data-tooltip="Zoom to"
          style={{
            position: "absolute",
            right: 6,
            top: "50%",
            transform: "translateY(-50%)",
            background: "none",
            border: "none",
            cursor: "pointer",
            color: "var(--text-tertiary)",
            display: "flex",
            padding: 0,
          }}
        >
          <MagnifyingGlassPlusIcon width={13} height={13} />
        </button>
      )}
    </>
  );
}
