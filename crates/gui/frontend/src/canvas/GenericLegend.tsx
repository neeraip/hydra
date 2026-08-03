// ── Engine-generic legend ─────────────────────────────────────────────────────
// The catalog-driven counterpart of `Legend`: renders one section per element
// class with a variable picker, a ramp bar, and the per-run min/max — all of
// it engine-authored data (`ResultMeta.generic`). No engine names, variable
// ids, units, or ranges are known here; the engine's catalog described them
// and this component draws exactly what it was given, matching the wds
// legend's glass look and bottom-left placement.

import type { CSSProperties } from "react";
import type { GenericResultMeta, GenericVariable } from "../hooks/results";

export type GenericClassKey = "point" | "polyline" | "region";

/** Selected variable id per element class ("" = class hidden/none). */
export type GenericSelection = Record<GenericClassKey, string>;

const SECTION_LABEL_STYLE: CSSProperties = {
  fontSize: "var(--text-xs)",
  fontWeight: 600,
  color: "var(--text-secondary)",
  marginBottom: 5,
};

const SELECT_STYLE: CSSProperties = {
  width: "100%",
  padding: "3px 4px",
  borderRadius: 6,
  border: "1px solid var(--border)",
  background: "var(--bg-card)",
  color: "var(--text-primary)",
  fontSize: "var(--text-xs)",
  fontFamily: "var(--font-ui)",
  boxSizing: "border-box",
  marginBottom: 6,
};

/** CSS gradient for a ramp hint — mirrors `genericRgba`'s palettes. */
function rampGradient(ramp: GenericVariable["ramp"]): string {
  if (ramp === "diverging") {
    return "linear-gradient(90deg, rgb(37,99,235), rgb(203,213,225), rgb(201,64,64))";
  }
  if (ramp === "banded") {
    // Five hard steps, good → excessive (BAND_STEPS in colorUtils).
    const steps = [
      "rgb(61,175,117)",
      "rgb(163,190,84)",
      "rgb(212,160,23)",
      "rgb(201,120,64)",
      "rgb(201,64,64)",
    ];
    const stops = steps
      .map((c, i) => `${c} ${i * 20}% ${(i + 1) * 20}%`)
      .join(", ");
    return `linear-gradient(90deg, ${stops})`;
  }
  return "linear-gradient(90deg, rgb(166,200,240), rgb(21,74,158))";
}

/** Compact numeric label: 3 significant digits, no scientific noise. */
function fmt(v: number): string {
  if (!Number.isFinite(v)) return "—";
  const a = Math.abs(v);
  if (a >= 1000) return Math.round(v).toLocaleString();
  if (a >= 10) return v.toFixed(1);
  return v.toFixed(2);
}

function RampBar({ variable }: { variable: GenericVariable }) {
  return (
    <div>
      <div
        style={{
          height: 8,
          borderRadius: 4,
          background: rampGradient(variable.ramp),
        }}
      />
      <div
        className="mono"
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontSize: "var(--text-2xs)",
          color: "var(--text-tertiary)",
          marginTop: 3,
        }}
      >
        <span>{fmt(variable.min)}</span>
        <span>{fmt(variable.max)}</span>
      </div>
    </div>
  );
}

function ClassSection({
  title,
  variables,
  selectedId,
  onSelect,
}: {
  title: string;
  variables: GenericVariable[];
  selectedId: string;
  onSelect: (id: string) => void;
}) {
  if (variables.length === 0) return null;
  const selected = variables.find((v) => v.id === selectedId) ?? variables[0];
  return (
    <div>
      <div style={SECTION_LABEL_STYLE}>{title}</div>
      <select
        style={SELECT_STYLE}
        value={selected.id}
        onChange={(e) => onSelect(e.target.value)}
        aria-label={`${title} variable`}
      >
        {variables.map((v) => (
          <option key={v.id} value={v.id}>
            {v.unit ? `${v.label} (${v.unit})` : v.label}
          </option>
        ))}
      </select>
      <RampBar variable={selected} />
    </div>
  );
}

/**
 * Legend for engines whose results are variable-keyed. Section titles are
 * element-class names (the one neutral vocabulary this layer owns);
 * everything inside each section is engine-authored.
 */
export function GenericLegend({
  meta,
  hasRegions,
  selection,
  onSelect,
}: {
  meta: GenericResultMeta;
  /** Whether the canvas is showing region polygons — hides the region
   * section for models without areal elements. */
  hasRegions: boolean;
  selection: GenericSelection;
  onSelect: (cls: GenericClassKey, id: string) => void;
}) {
  return (
    <div
      style={{
        position: "absolute",
        bottom: 14,
        left: "calc(var(--rail-effective-w, 0px) + 16px)",
        zIndex: 30,
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-start",
        transition: "left var(--rail-transition)",
      }}
    >
      <div
        className="legend-glass legend-glass--raised"
        style={{
          backdropFilter: "blur(20px) saturate(160%)",
          WebkitBackdropFilter: "blur(20px) saturate(160%)",
          borderRadius: 10,
          padding: "10px 14px",
          width: 200,
          display: "flex",
          flexDirection: "column",
          gap: 12,
        }}
      >
        <ClassSection
          title="Nodes"
          variables={meta.pointVars}
          selectedId={selection.point}
          onSelect={(id) => onSelect("point", id)}
        />
        <ClassSection
          title="Links"
          variables={meta.polylineVars}
          selectedId={selection.polyline}
          onSelect={(id) => onSelect("polyline", id)}
        />
        {hasRegions && (
          <ClassSection
            title="Regions"
            variables={meta.regionVars}
            selectedId={selection.region}
            onSelect={(id) => onSelect("region", id)}
          />
        )}
      </div>
    </div>
  );
}
