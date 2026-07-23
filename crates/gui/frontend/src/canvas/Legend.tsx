/**
 * Legend — colour-scale legend + variable picker overlay for the canvas.
 *
 * Renders as a small persistent control bar at the bottom-left of the canvas
 * with two always-visible dropdown buttons for switching the active node and
 * link variables (Pressure, Head, Flow, Velocity, etc.) — no expand step
 * required. A separate swatch button opens a details popover with the full
 * gradient bars, min/max labels, and optional threshold editing when
 * colorMode is "threshold".
 */

import { ChevronUpDownIcon, PlayIcon } from "@heroicons/react/16/solid";
import React, { type CSSProperties, useEffect, useRef, useState } from "react";
import {
  fromDisplay,
  type Quantity,
  toDisplay,
  unitLabel,
  useUnitSystem,
} from "../units";
import {
  DIVERGING_GRADIENT_CSS,
  FLOW_GRADIENT_CSS,
  LINK_RISK_GRADIENT_CSS,
  PRESSURE_GRADIENT_CSS,
  QUALITY_GRADIENT_CSS,
  SEQ_GRADIENT_CSS,
  VELOCITY_GRADIENT_CSS,
} from "./colors";
import { LINK_HEADLOSS_MAX } from "./MapCanvas/colorUtils";
import type { LinkVariable, NodeVariable } from "./types";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface LegendThresholds {
  pressure: { low: number; required: number; high: number };
  velocity: { low: number; target: number; high: number };
  flow: { low: number; target: number; high: number };
}

/** Scenario-comparison legend state: baseline caption + Δ ramp bounds. */
export interface LegendCompare {
  /** Display name of the baseline ("Base model" or the scenario name). */
  baselineName: string;
  /** Max |Δ| (SI) for the active node variable. */
  nodeMaxAbs: number;
  /** Max |Δ| (SI) for the active link variable; `null` when the link
   * variable is "status" (categorical — not comparable). */
  linkMaxAbs: number | null;
}

interface LegendProps {
  nodeVar: NodeVariable;
  setNodeVar: (v: NodeVariable) => void;
  linkVar: LinkVariable;
  setLinkVar: (v: LinkVariable) => void;
  /** User preference: animate the Flow/Velocity pulse effect. */
  linkAnimation: boolean;
  setLinkAnimation: (v: boolean) => void;
  /** App "Reduce motion" accessibility setting; forces animation off. */
  reducedMotion: boolean;
  qualityMode: string;
  headMin: number;
  headMax: number;
  demandMin: number;
  demandMax: number;
  flowMax: number;
  qualityMin: number;
  qualityMax: number;
  colorMode: "relative" | "threshold";
  thresholds: LegendThresholds;
  onColorModeChange: (mode: "relative" | "threshold") => void;
  onThresholdsChange: (t: LegendThresholds) => void;
  /** Non-null while scenario comparison is active: ramps become the
   * diverging Δ scale and the threshold editor is hidden. */
  compare?: LegendCompare | null;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// The node gradient does not depend on colorMode: the map colours pressure
// with the same red/amber/green/blue bands in both modes (pressureRgba — only
// the cutoff values change in threshold mode), and head/demand/quality keep
// their sequential ramps regardless of mode.
function nodeGradient(nodeVar: NodeVariable): string {
  if (nodeVar === "pressure") return PRESSURE_GRADIENT_CSS;
  if (nodeVar === "quality") return QUALITY_GRADIENT_CSS;
  return SEQ_GRADIENT_CSS;
}

function linkGradient(
  linkVar: LinkVariable,
  colorMode: "relative" | "threshold",
): string | null {
  if (linkVar === "status") return null;
  // Headloss and quality have no threshold bands — their ramps are the same
  // in both colour modes (headlossRgba / linkQualityRgba on the map).
  if (linkVar === "headloss") return SEQ_GRADIENT_CSS;
  if (linkVar === "quality") return QUALITY_GRADIENT_CSS;
  // Threshold mode renders four discrete bands on the map
  // (velocityRgba / flowMagnitudeRgba): green → amber → orange → red.
  if (colorMode === "threshold") return LINK_RISK_GRADIENT_CSS;
  if (linkVar === "flow") return FLOW_GRADIENT_CSS;
  if (linkVar === "velocity") return VELOCITY_GRADIENT_CSS;
  return SEQ_GRADIENT_CSS;
}

// ── Sub-components ────────────────────────────────────────────────────────────

function Ramp({
  gradient,
  min,
  max,
}: {
  gradient: string;
  min: number;
  max: number;
}) {
  return (
    <div>
      <div
        style={{
          height: 10,
          borderRadius: 5,
          background: gradient,
          marginBottom: 4,
        }}
      />
      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <span
          className="mono"
          style={{ fontSize: 10, color: "var(--text-tertiary)" }}
        >
          {min.toFixed(1)}
        </span>
        <span
          className="mono"
          style={{ fontSize: 10, color: "var(--text-tertiary)" }}
        >
          {max.toFixed(1)}
        </span>
      </div>
    </div>
  );
}

/** Compact delta-bound label: whole numbers when large, more precision when
 * the differences are small (Δ ranges are often well under 1 display unit). */
function fmtDelta(v: number): string {
  const a = Math.abs(v);
  if (a >= 100) return v.toFixed(0);
  if (a >= 1) return v.toFixed(1);
  return v.toPrecision(2);
}

const RAMP_LABEL_STYLE: CSSProperties = {
  fontSize: 10,
  color: "var(--text-tertiary)",
};

/** Diverging Δ ramp with -max … 0 … +max labels (display units). */
function DeltaRamp({ maxAbs }: { maxAbs: number }) {
  return (
    <div>
      <div
        style={{
          height: 10,
          borderRadius: 5,
          background: DIVERGING_GRADIENT_CSS,
          marginBottom: 4,
        }}
      />
      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <span className="mono" style={RAMP_LABEL_STYLE}>
          -{fmtDelta(maxAbs)}
        </span>
        <span className="mono" style={RAMP_LABEL_STYLE}>
          0
        </span>
        <span className="mono" style={RAMP_LABEL_STYLE}>
          +{fmtDelta(maxAbs)}
        </span>
      </div>
    </div>
  );
}

/** Discrete link-status legend colours — must match `statusRgba` in
 * MapCanvas/colorUtils. */
const STATUS_LEGEND = [
  { color: "#78a0b9", label: "Open" },
  { color: "#d4a017", label: "Active" },
  { color: "#c94040", label: "Closed" },
];

function StatusSwatches() {
  return (
    <div>
      <div style={{ display: "flex", gap: 10 }}>
        {STATUS_LEGEND.map(({ color, label }) => (
          <div
            key={label}
            style={{ display: "flex", alignItems: "center", gap: 5 }}
          >
            <div
              style={{
                width: 12,
                height: 12,
                borderRadius: 3,
                background: color,
              }}
            />
            <span style={{ fontSize: 10, color: "var(--text-secondary)" }}>
              {label}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

const inputStyle: CSSProperties = {
  width: "100%",
  padding: "4px 6px",
  borderRadius: 6,
  color: "var(--text-primary)",
  fontSize: 10,
  boxSizing: "border-box",
};

/** Style for the "< low · target · > high" annotation under a ramp. */
const THRESHOLD_ANNOTATION_STYLE: CSSProperties = {
  marginTop: 5,
  fontSize: 10,
  color: "var(--text-tertiary)",
  lineHeight: 1.5,
};

// ── Legend component ──────────────────────────────────────────────────────────

const SECTION_LABEL_STYLE: React.CSSProperties = {
  fontSize: 10,
  fontWeight: 600,
  color: "var(--text-secondary)",
  marginBottom: 5,
};

const PICKER_BTN_STYLE: React.CSSProperties = {
  width: "auto",
  height: 26,
  padding: "0 8px",
  gap: 4,
  display: "flex",
  alignItems: "center",
  fontSize: 11,
  fontWeight: 600,
  fontFamily: "var(--font-ui)",
  color: "var(--text-primary)",
  whiteSpace: "nowrap",
};

const PICKER_LIST_STYLE: React.CSSProperties = {
  position: "absolute",
  bottom: "calc(100% + 6px)",
  left: 0,
  backdropFilter: "blur(20px) saturate(160%)",
  WebkitBackdropFilter: "blur(20px) saturate(160%)",
  borderRadius: 8,
  overflow: "hidden",
  minWidth: 130,
  zIndex: 40,
};

interface PickerOption<T extends string> {
  value: T;
  label: string;
}

/** Always-visible dropdown button for switching a canvas variable — mirrors
 * the basemap/CRS picker pattern used in the canvas toolbar. */
function PickerButton<T extends string>({
  value,
  options,
  isOpen,
  onToggle,
  onSelect,
}: {
  value: T;
  options: PickerOption<T>[];
  isOpen: boolean;
  onToggle: () => void;
  onSelect: (v: T) => void;
}) {
  const current = options.find((o) => o.value === value);
  return (
    <div style={{ position: "relative" }}>
      <button
        type="button"
        className="tool-btn"
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
        style={PICKER_BTN_STYLE}
      >
        {current?.label ?? value}
        <ChevronUpDownIcon style={{ width: 12, height: 12 }} />
      </button>
      {isOpen && (
        <div
          className="legend-glass legend-glass--raised"
          style={PICKER_LIST_STYLE}
        >
          {options.map((o) => (
            <button
              type="button"
              key={o.value}
              onClick={() => onSelect(o.value)}
              style={{
                display: "block",
                width: "100%",
                padding: "6px 10px",
                border: "none",
                background:
                  o.value === value ? "rgba(74,144,217,0.22)" : "transparent",
                color:
                  o.value === value ? "var(--accent)" : "var(--text-secondary)",
                cursor: "pointer",
                fontSize: 11,
                textAlign: "left",
                fontFamily: "var(--font-ui)",
              }}
            >
              {o.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** One titled row of the threshold editor: three inputs + footer labels.
 * `fields` are both the value keys and the input placeholders. */
function ThresholdEditorSection<K extends string>({
  title,
  fields,
  footLabels,
  values,
  onChange,
}: {
  title: string;
  fields: readonly K[];
  footLabels: readonly string[];
  values: Record<K, string>;
  onChange: (key: K, value: string) => void;
}) {
  return (
    <div>
      <div style={SECTION_LABEL_STYLE}>{title}</div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          gap: 4,
        }}
      >
        {fields.map((f) => (
          <input
            key={f}
            value={values[f]}
            onChange={(e) => onChange(f, e.target.value)}
            placeholder={f}
            style={inputStyle}
          />
        ))}
      </div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          marginTop: 2,
        }}
      >
        {footLabels.map((l) => (
          <span key={l} style={{ fontSize: 9, color: "var(--text-disabled)" }}>
            {l}
          </span>
        ))}
      </div>
    </div>
  );
}

export function Legend({
  nodeVar,
  setNodeVar,
  linkVar,
  setLinkVar,
  linkAnimation,
  setLinkAnimation,
  reducedMotion,
  qualityMode,
  headMin,
  headMax,
  demandMin,
  demandMax,
  flowMax,
  qualityMin,
  qualityMax,
  colorMode,
  thresholds,
  onColorModeChange,
  onThresholdsChange,
  compare = null,
}: LegendProps) {
  const sys = useUnitSystem();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [nodePickerOpen, setNodePickerOpen] = useState(false);
  const [linkPickerOpen, setLinkPickerOpen] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [editing, setEditing] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // Thresholds are stored SI; the editor and all labels present them in the
  // active display system and convert entered values back on Apply.
  const disp = (v: number, q: Quantity) =>
    Number(toDisplay(v, q, sys).toFixed(2));

  // Close any open dropdown/popover when clicking outside the legend.
  useEffect(() => {
    function onPointerDown(e: PointerEvent) {
      if (rootRef.current?.contains(e.target as Node)) return;
      setDetailsOpen(false);
      setNodePickerOpen(false);
      setLinkPickerOpen(false);
    }
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, []);

  // Local editors — strings so the user can type freely
  const [pressEd, setPressEd] = useState({ low: "", required: "", high: "" });
  const [velEd, setVelEd] = useState({ low: "", target: "", high: "" });
  const [flowEd, setFlowEd] = useState({ low: "", target: "", high: "" });

  // Sync editors whenever thresholds change from outside (values shown in
  // the active display system).
  // biome-ignore lint/correctness/useExhaustiveDependencies: `disp` derives only from `sys`, which is a dependency.
  useEffect(() => {
    setPressEd({
      low: String(disp(thresholds.pressure.low, "pressure")),
      required: String(disp(thresholds.pressure.required, "pressure")),
      high: String(disp(thresholds.pressure.high, "pressure")),
    });
    setVelEd({
      low: String(disp(thresholds.velocity.low, "velocity")),
      target: String(disp(thresholds.velocity.target, "velocity")),
      high: String(disp(thresholds.velocity.high, "velocity")),
    });
    setFlowEd({
      low: String(disp(thresholds.flow.low, "flow")),
      target: String(disp(thresholds.flow.target, "flow")),
      high: String(disp(thresholds.flow.high, "flow")),
    });
  }, [thresholds, sys]);

  function applyEdits() {
    // Entered values are in display units — convert back to SI for storage.
    onThresholdsChange({
      pressure: {
        low: fromDisplay(Number(pressEd.low), "pressure", sys),
        required: fromDisplay(Number(pressEd.required), "pressure", sys),
        high: fromDisplay(Number(pressEd.high), "pressure", sys),
      },
      velocity: {
        low: fromDisplay(Number(velEd.low), "velocity", sys),
        target: fromDisplay(Number(velEd.target), "velocity", sys),
        high: fromDisplay(Number(velEd.high), "velocity", sys),
      },
      flow: {
        low: fromDisplay(Number(flowEd.low), "flow", sys),
        target: fromDisplay(Number(flowEd.target), "flow", sys),
        high: fromDisplay(Number(flowEd.high), "flow", sys),
      },
    });
    setEditing(false);
  }

  // Colour ramps — the diverging Δ ramp replaces both while comparing
  // (status keeps its discrete swatches in either mode).
  const ng = compare ? DIVERGING_GRADIENT_CSS : nodeGradient(nodeVar);
  const lg = compare
    ? DIVERGING_GRADIENT_CSS
    : linkGradient(linkVar, colorMode);

  // Display quantities for ramp min/max labels (null = unitless).
  const nodeQty: Quantity | null =
    nodeVar === "pressure"
      ? "pressure"
      : nodeVar === "head"
        ? "head"
        : nodeVar === "demand"
          ? "demand"
          : null;
  const linkQty: Quantity | null =
    linkVar === "flow"
      ? "flow"
      : linkVar === "velocity"
        ? "velocity"
        : linkVar === "headloss"
          ? "headloss"
          : null;

  // Label range for node variable
  const [nMin, nMax] = (() => {
    if (colorMode === "threshold") {
      if (nodeVar === "pressure")
        return [thresholds.pressure.low, thresholds.pressure.high];
    }
    if (nodeVar === "pressure") return [0, 60];
    if (nodeVar === "head") return [headMin, headMax];
    if (nodeVar === "demand") return [demandMin, demandMax];
    return [qualityMin, qualityMax];
  })();

  // Label range for link variable
  const [lMin, lMax] = (() => {
    if (linkVar === "status") return [0, 1];
    if (colorMode === "threshold") {
      if (linkVar === "flow")
        return [thresholds.flow.low, thresholds.flow.high];
      if (linkVar === "velocity")
        return [thresholds.velocity.low, thresholds.velocity.high];
    }
    if (linkVar === "flow") return [0, flowMax];
    if (linkVar === "velocity") return [0, 1.5];
    // Fixed cap matching headlossRgba's scale on the map.
    if (linkVar === "headloss") return [0, LINK_HEADLOSS_MAX];
    if (linkVar === "quality") return [qualityMin, qualityMax];
    return [0, 1];
  })();

  // Show threshold annotations in threshold mode (never while comparing —
  // Δ colours ignore threshold bands entirely).
  const showPressureAnnotations =
    !compare && colorMode === "threshold" && nodeVar === "pressure";
  const showVelAnnotations =
    !compare && colorMode === "threshold" && linkVar === "velocity";
  const showFlowAnnotations =
    !compare && colorMode === "threshold" && linkVar === "flow";

  const nodeOptions: PickerOption<NodeVariable>[] = [
    { value: "pressure", label: `Pressure (${unitLabel("pressure", sys)})` },
    { value: "head", label: `Head (${unitLabel("head", sys)})` },
    { value: "demand", label: `Demand (${unitLabel("demand", sys)})` },
    ...(qualityMode !== "none"
      ? [{ value: "quality" as const, label: "Quality" }]
      : []),
  ];
  const linkOptions: PickerOption<LinkVariable>[] = [
    { value: "flow", label: `Flow (${unitLabel("flow", sys)})` },
    { value: "velocity", label: `Velocity (${unitLabel("velocity", sys)})` },
    { value: "headloss", label: `Headloss (${unitLabel("headloss", sys)})` },
    { value: "status", label: "Status" },
    // Mirrors the node quality gating: only offered when the loaded result
    // actually has quality data.
    ...(qualityMode !== "none"
      ? [{ value: "quality" as const, label: "Quality" }]
      : []),
  ];

  return (
    <div
      ref={rootRef}
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
      {/* ── Popover: gradient ramps, threshold editor, colour-mode toggle ── */}
      {detailsOpen && (
        <div
          className="legend-glass legend-glass--raised"
          style={{
            marginBottom: 8,
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
          {/* Comparison caption — which baseline the Δ colours are against */}
          {compare && (
            <div
              style={{
                fontSize: 10,
                fontWeight: 600,
                color: "var(--text-secondary)",
              }}
            >
              Δ vs {compare.baselineName}
            </div>
          )}

          {/* Node variable ramp — variable is switched via the picker below */}
          <div>
            <div style={SECTION_LABEL_STYLE}>
              {compare ? "Δ " : ""}
              {nodeOptions.find((o) => o.value === nodeVar)?.label}
            </div>
            {compare ? (
              <DeltaRamp
                maxAbs={
                  nodeQty
                    ? toDisplay(compare.nodeMaxAbs, nodeQty, sys)
                    : compare.nodeMaxAbs
                }
              />
            ) : (
              <Ramp
                gradient={ng}
                min={nodeQty ? toDisplay(nMin, nodeQty, sys) : nMin}
                max={nodeQty ? toDisplay(nMax, nodeQty, sys) : nMax}
              />
            )}
            {showPressureAnnotations && (
              <div style={THRESHOLD_ANNOTATION_STYLE}>
                {`< ${disp(thresholds.pressure.low, "pressure")} low · ${disp(thresholds.pressure.required, "pressure")} required · > ${disp(thresholds.pressure.high, "pressure")} high`}
              </div>
            )}
          </div>

          {/* Link variable ramp — variable is switched via the picker below */}
          <div>
            <div style={SECTION_LABEL_STYLE}>
              {compare && linkVar !== "status" ? "Δ " : ""}
              {linkOptions.find((o) => o.value === linkVar)?.label}
            </div>
            {linkVar === "status" ? (
              <>
                <StatusSwatches />
                {compare && (
                  <div style={THRESHOLD_ANNOTATION_STYLE}>
                    Status is not comparable — showing current values.
                  </div>
                )}
              </>
            ) : compare ? (
              <DeltaRamp
                maxAbs={
                  compare.linkMaxAbs == null
                    ? 0
                    : linkQty
                      ? toDisplay(compare.linkMaxAbs, linkQty, sys)
                      : compare.linkMaxAbs
                }
              />
            ) : (
              <>
                <Ramp
                  gradient={lg ?? SEQ_GRADIENT_CSS}
                  min={linkQty ? toDisplay(lMin, linkQty, sys) : lMin}
                  max={linkQty ? toDisplay(lMax, linkQty, sys) : lMax}
                />
                {(showVelAnnotations || showFlowAnnotations) &&
                  (() => {
                    const [t, q] =
                      linkVar === "velocity"
                        ? ([thresholds.velocity, "velocity"] as const)
                        : ([thresholds.flow, "flow"] as const);
                    return (
                      <div style={THRESHOLD_ANNOTATION_STYLE}>
                        {`< ${disp(t.low, q)} low · ${disp(t.target, q)} target · > ${disp(t.high, q)} high`}
                      </div>
                    );
                  })()}
              </>
            )}
          </div>

          {/* Thresholds don't apply to Δ colours — editor hidden while comparing */}
          {compare && colorMode === "threshold" && (
            <div style={THRESHOLD_ANNOTATION_STYLE}>
              Thresholds don't apply while comparing.
            </div>
          )}

          {/* Threshold editor (only in threshold mode) */}
          {!compare && colorMode === "threshold" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <button
                type="button"
                onClick={() => setEditing((v) => !v)}
                className={`legend-btn-secondary${editing ? " legend-btn-secondary--active" : ""}`}
                style={{
                  width: "100%",
                  padding: "5px 8px",
                  borderRadius: 6,
                  color: "var(--text-secondary)",
                  fontSize: 10,
                  cursor: "pointer",
                  fontFamily: "var(--font-ui)",
                }}
              >
                {editing ? "Close editor" : "Edit thresholds"}
              </button>

              {editing && (
                <div
                  style={{ display: "flex", flexDirection: "column", gap: 10 }}
                >
                  <ThresholdEditorSection
                    title={`Pressure (${unitLabel("pressure", sys)})`}
                    fields={["low", "required", "high"]}
                    footLabels={["low", "req'd", "high"]}
                    values={pressEd}
                    onChange={(k, v) => setPressEd((p) => ({ ...p, [k]: v }))}
                  />
                  <ThresholdEditorSection
                    title={`Velocity (${unitLabel("velocity", sys)})`}
                    fields={["low", "target", "high"]}
                    footLabels={["low", "target", "high"]}
                    values={velEd}
                    onChange={(k, v) => setVelEd((p) => ({ ...p, [k]: v }))}
                  />
                  <ThresholdEditorSection
                    title={`Flow (${unitLabel("flow", sys)})`}
                    fields={["low", "target", "high"]}
                    footLabels={["low", "target", "high"]}
                    values={flowEd}
                    onChange={(k, v) => setFlowEd((p) => ({ ...p, [k]: v }))}
                  />

                  <button
                    type="button"
                    onClick={applyEdits}
                    style={{
                      width: "100%",
                      padding: "5px 8px",
                      borderRadius: 6,
                      border: "1px solid rgba(74,144,217,0.4)",
                      background: "rgba(74,144,217,0.22)",
                      color: "var(--text-primary)",
                      fontSize: 10,
                      cursor: "pointer",
                      fontFamily: "var(--font-ui)",
                    }}
                  >
                    Apply
                  </button>
                </div>
              )}
            </div>
          )}

          {/* Mode toggle — inapplicable (disabled) while comparing */}
          <div style={{ display: "flex", gap: 6 }}>
            {(["relative", "threshold"] as const).map((m) => (
              <button
                type="button"
                key={m}
                disabled={!!compare}
                onClick={() => {
                  onColorModeChange(m);
                  if (m === "relative") setEditing(false);
                }}
                className={`legend-btn-secondary${colorMode === m ? " legend-btn-secondary--active" : ""}`}
                style={{
                  flex: 1,
                  padding: "5px 8px",
                  borderRadius: 6,
                  color:
                    colorMode === m ? "var(--accent)" : "var(--text-secondary)",
                  fontSize: 10,
                  cursor: compare ? "default" : "pointer",
                  opacity: compare ? 0.5 : 1,
                  fontFamily: "var(--font-ui)",
                }}
              >
                {m.charAt(0).toUpperCase() + m.slice(1)}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* ── Persistent control bar: variable pickers + colour-scale toggle ── */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: hover-only styling; all interactive children (PickerButton, toggle) are already focusable/clickable. */}
      <div
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        className={`legend-glass${
          hovered || detailsOpen || nodePickerOpen || linkPickerOpen
            ? " legend-glass--raised"
            : ""
        }`}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 4,
          padding: 4,
          minHeight: 32,
          borderRadius: 20,
          backdropFilter: "blur(20px) saturate(160%)",
          WebkitBackdropFilter: "blur(20px) saturate(160%)",
        }}
      >
        {/* Colour scale / thresholds toggle — separate from variable switching */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setDetailsOpen((v) => !v);
            setNodePickerOpen(false);
            setLinkPickerOpen(false);
          }}
          title="Colour scale & thresholds"
          data-tooltip="Colour scale & thresholds"
          data-tooltip-pos="top"
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            border: "none",
            background: "transparent",
            cursor: "pointer",
            padding: "4px 6px 4px 4px",
            borderRadius: 16,
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
            {/* Node ramp swatch */}
            <div
              style={{ width: 28, height: 5, borderRadius: 3, background: ng }}
            />
            {/* Link ramp swatch (or discrete status swatches) */}
            {linkVar === "status" ? (
              <div style={{ display: "flex", gap: 2 }}>
                {STATUS_LEGEND.map(({ color }) => (
                  <div
                    key={color}
                    style={{
                      width: 8,
                      height: 5,
                      borderRadius: 3,
                      background: color,
                    }}
                  />
                ))}
              </div>
            ) : (
              <div
                style={{
                  width: 28,
                  height: 5,
                  borderRadius: 3,
                  background: lg ?? SEQ_GRADIENT_CSS,
                }}
              />
            )}
          </div>
          <ChevronUpDownIcon
            style={{ width: 10, height: 10, color: "var(--text-tertiary)" }}
          />
        </button>
        <div className="tool-divider" />
        <PickerButton
          value={nodeVar}
          options={nodeOptions}
          isOpen={nodePickerOpen}
          onToggle={() => {
            setNodePickerOpen((v) => !v);
            setLinkPickerOpen(false);
            setDetailsOpen(false);
          }}
          onSelect={(v) => {
            setNodeVar(v);
            setNodePickerOpen(false);
          }}
        />
        <PickerButton
          value={linkVar}
          options={linkOptions}
          isOpen={linkPickerOpen}
          onToggle={() => {
            setLinkPickerOpen((v) => !v);
            setNodePickerOpen(false);
            setDetailsOpen(false);
          }}
          onSelect={(v) => {
            setLinkVar(v);
            setLinkPickerOpen(false);
          }}
        />
        {(() => {
          // Animation applies to Flow and Velocity only; "Reduce motion"
          // (Settings → Accessibility) always wins over the user toggle, and
          // comparison mode forces static Δ colours.
          const animatable = linkVar === "flow" || linkVar === "velocity";
          const disabled = reducedMotion || !animatable || !!compare;
          const tooltip = reducedMotion
            ? "Animation off — Reduce motion is enabled in Settings"
            : compare
              ? "Animation off while comparing scenarios"
              : !animatable
                ? "Animation applies to Flow and Velocity"
                : linkAnimation
                  ? "Pause link animation"
                  : "Play link animation";
          const active = linkAnimation && !disabled;
          return (
            <button
              type="button"
              className="tool-btn"
              disabled={disabled}
              onClick={(e) => {
                e.stopPropagation();
                setLinkAnimation(!linkAnimation);
              }}
              title={tooltip}
              data-tooltip={tooltip}
              data-tooltip-pos="top"
              style={{
                ...PICKER_BTN_STYLE,
                padding: "4px 6px",
                color: active
                  ? "var(--accent)"
                  : disabled
                    ? "var(--text-tertiary)"
                    : "var(--text-secondary)",
                opacity: disabled ? 0.5 : 1,
                cursor: disabled ? "default" : "pointer",
              }}
            >
              <PlayIcon style={{ width: 12, height: 12 }} />
            </button>
          );
        })()}
      </div>
    </div>
  );
}
