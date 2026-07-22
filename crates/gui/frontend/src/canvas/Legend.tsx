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

import { ChevronUpDownIcon } from "@heroicons/react/16/solid";
import React, { type CSSProperties, useEffect, useRef, useState } from "react";
import {
  FLOW_GRADIENT_CSS,
  PRESSURE_GRADIENT_CSS,
  QUALITY_GRADIENT_CSS,
  RISK_GRADIENT_CSS,
  SEQ_GRADIENT_CSS,
  VELOCITY_GRADIENT_CSS,
} from "./colors";
import type { LinkVariable, NodeVariable } from "./types";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface LegendThresholds {
  pressure: { low: number; required: number; high: number };
  velocity: { low: number; target: number; high: number };
  flow: { low: number; target: number; high: number };
}

interface LegendProps {
  nodeVar: NodeVariable;
  setNodeVar: (v: NodeVariable) => void;
  linkVar: LinkVariable;
  setLinkVar: (v: LinkVariable) => void;
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
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function nodeGradient(
  nodeVar: NodeVariable,
  colorMode: "relative" | "threshold",
): string {
  if (colorMode === "threshold") return RISK_GRADIENT_CSS;
  if (nodeVar === "pressure") return PRESSURE_GRADIENT_CSS;
  if (nodeVar === "quality") return QUALITY_GRADIENT_CSS;
  return SEQ_GRADIENT_CSS;
}

function linkGradient(
  linkVar: LinkVariable,
  colorMode: "relative" | "threshold",
): string | null {
  if (linkVar === "status") return null;
  if (colorMode === "threshold") return RISK_GRADIENT_CSS;
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
  label: string;
  gradient: string;
  min: number;
  max: number;
  dot: boolean;
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

function StatusSwatches() {
  return (
    <div>
      <div style={{ display: "flex", gap: 10 }}>
        {[
          { color: "#78a0b9", label: "Open" },
          { color: "#d4a017", label: "Active" },
          { color: "#c94040", label: "Closed" },
        ].map(({ color, label }) => (
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

export function Legend({
  nodeVar,
  setNodeVar,
  linkVar,
  setLinkVar,
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
}: LegendProps) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [nodePickerOpen, setNodePickerOpen] = useState(false);
  const [linkPickerOpen, setLinkPickerOpen] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [editing, setEditing] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

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

  // Sync editors whenever thresholds change from outside
  useEffect(() => {
    setPressEd({
      low: String(thresholds.pressure.low),
      required: String(thresholds.pressure.required),
      high: String(thresholds.pressure.high),
    });
    setVelEd({
      low: String(thresholds.velocity.low),
      target: String(thresholds.velocity.target),
      high: String(thresholds.velocity.high),
    });
    setFlowEd({
      low: String(thresholds.flow.low),
      target: String(thresholds.flow.target),
      high: String(thresholds.flow.high),
    });
  }, [thresholds]);

  function applyEdits() {
    onThresholdsChange({
      pressure: {
        low: Number(pressEd.low),
        required: Number(pressEd.required),
        high: Number(pressEd.high),
      },
      velocity: {
        low: Number(velEd.low),
        target: Number(velEd.target),
        high: Number(velEd.high),
      },
      flow: {
        low: Number(flowEd.low),
        target: Number(flowEd.target),
        high: Number(flowEd.high),
      },
    });
    setEditing(false);
  }

  // Colour ramps
  const ng = nodeGradient(nodeVar, colorMode);
  const lg = linkGradient(linkVar, colorMode);

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
    return [0, 1];
  })();

  // Show threshold annotations in threshold mode
  const showPressureAnnotations =
    colorMode === "threshold" && nodeVar === "pressure";
  const showVelAnnotations =
    colorMode === "threshold" && linkVar === "velocity";
  const showFlowAnnotations = colorMode === "threshold" && linkVar === "flow";

  const nodeOptions: PickerOption<NodeVariable>[] = [
    { value: "pressure", label: "Pressure (m)" },
    { value: "head", label: "Head (m)" },
    { value: "demand", label: "Demand (L/s)" },
    ...(qualityMode !== "none"
      ? [{ value: "quality" as const, label: "Quality" }]
      : []),
  ];
  const linkOptions: PickerOption<LinkVariable>[] = [
    { value: "flow", label: "Flow (L/s)" },
    { value: "velocity", label: "Velocity (m/s)" },
    { value: "status", label: "Status" },
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
          {/* Node variable ramp — variable is switched via the picker below */}
          <div>
            <div style={SECTION_LABEL_STYLE}>
              {nodeOptions.find((o) => o.value === nodeVar)?.label}
            </div>
            <Ramp label="" gradient={ng} min={nMin} max={nMax} dot={false} />
            {showPressureAnnotations && (
              <div
                style={{
                  marginTop: 5,
                  fontSize: 10,
                  color: "var(--text-tertiary)",
                  lineHeight: 1.5,
                }}
              >
                {`< ${thresholds.pressure.low} low · ${thresholds.pressure.required} required · > ${thresholds.pressure.high} high`}
              </div>
            )}
          </div>

          {/* Link variable ramp — variable is switched via the picker below */}
          <div>
            <div style={SECTION_LABEL_STYLE}>
              {linkOptions.find((o) => o.value === linkVar)?.label}
            </div>
            {linkVar === "status" ? (
              <StatusSwatches />
            ) : (
              <>
                <Ramp
                  label=""
                  gradient={lg ?? SEQ_GRADIENT_CSS}
                  min={lMin}
                  max={lMax}
                  dot={false}
                />
                {(showVelAnnotations || showFlowAnnotations) &&
                  (() => {
                    const t =
                      linkVar === "velocity"
                        ? thresholds.velocity
                        : thresholds.flow;
                    return (
                      <div
                        style={{
                          marginTop: 5,
                          fontSize: 10,
                          color: "var(--text-tertiary)",
                          lineHeight: 1.5,
                        }}
                      >
                        {`< ${t.low} low · ${t.target} target · > ${t.high} high`}
                      </div>
                    );
                  })()}
              </>
            )}
          </div>

          {/* Threshold editor (only in threshold mode) */}
          {colorMode === "threshold" && (
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
                  {/* Pressure */}
                  <div>
                    <div
                      style={{
                        fontSize: 10,
                        color: "var(--text-secondary)",
                        fontWeight: 600,
                        marginBottom: 5,
                      }}
                    >
                      Pressure (m)
                    </div>
                    <div
                      style={{
                        display: "grid",
                        gridTemplateColumns: "repeat(3, 1fr)",
                        gap: 4,
                      }}
                    >
                      <input
                        value={pressEd.low}
                        onChange={(e) =>
                          setPressEd((p) => ({ ...p, low: e.target.value }))
                        }
                        placeholder="low"
                        style={inputStyle}
                      />
                      <input
                        value={pressEd.required}
                        onChange={(e) =>
                          setPressEd((p) => ({
                            ...p,
                            required: e.target.value,
                          }))
                        }
                        placeholder="required"
                        style={inputStyle}
                      />
                      <input
                        value={pressEd.high}
                        onChange={(e) =>
                          setPressEd((p) => ({ ...p, high: e.target.value }))
                        }
                        placeholder="high"
                        style={inputStyle}
                      />
                    </div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        marginTop: 2,
                      }}
                    >
                      {["low", "req'd", "high"].map((l) => (
                        <span
                          key={l}
                          style={{ fontSize: 9, color: "var(--text-disabled)" }}
                        >
                          {l}
                        </span>
                      ))}
                    </div>
                  </div>

                  {/* Velocity */}
                  <div>
                    <div
                      style={{
                        fontSize: 10,
                        color: "var(--text-secondary)",
                        fontWeight: 600,
                        marginBottom: 5,
                      }}
                    >
                      Velocity (m/s)
                    </div>
                    <div
                      style={{
                        display: "grid",
                        gridTemplateColumns: "repeat(3, 1fr)",
                        gap: 4,
                      }}
                    >
                      <input
                        value={velEd.low}
                        onChange={(e) =>
                          setVelEd((p) => ({ ...p, low: e.target.value }))
                        }
                        placeholder="low"
                        style={inputStyle}
                      />
                      <input
                        value={velEd.target}
                        onChange={(e) =>
                          setVelEd((p) => ({ ...p, target: e.target.value }))
                        }
                        placeholder="target"
                        style={inputStyle}
                      />
                      <input
                        value={velEd.high}
                        onChange={(e) =>
                          setVelEd((p) => ({ ...p, high: e.target.value }))
                        }
                        placeholder="high"
                        style={inputStyle}
                      />
                    </div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        marginTop: 2,
                      }}
                    >
                      {["low", "target", "high"].map((l) => (
                        <span
                          key={l}
                          style={{ fontSize: 9, color: "var(--text-disabled)" }}
                        >
                          {l}
                        </span>
                      ))}
                    </div>
                  </div>

                  {/* Flow */}
                  <div>
                    <div
                      style={{
                        fontSize: 10,
                        color: "var(--text-secondary)",
                        fontWeight: 600,
                        marginBottom: 5,
                      }}
                    >
                      Flow (L/s)
                    </div>
                    <div
                      style={{
                        display: "grid",
                        gridTemplateColumns: "repeat(3, 1fr)",
                        gap: 4,
                      }}
                    >
                      <input
                        value={flowEd.low}
                        onChange={(e) =>
                          setFlowEd((p) => ({ ...p, low: e.target.value }))
                        }
                        placeholder="low"
                        style={inputStyle}
                      />
                      <input
                        value={flowEd.target}
                        onChange={(e) =>
                          setFlowEd((p) => ({ ...p, target: e.target.value }))
                        }
                        placeholder="target"
                        style={inputStyle}
                      />
                      <input
                        value={flowEd.high}
                        onChange={(e) =>
                          setFlowEd((p) => ({ ...p, high: e.target.value }))
                        }
                        placeholder="high"
                        style={inputStyle}
                      />
                    </div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        marginTop: 2,
                      }}
                    >
                      {["low", "target", "high"].map((l) => (
                        <span
                          key={l}
                          style={{ fontSize: 9, color: "var(--text-disabled)" }}
                        >
                          {l}
                        </span>
                      ))}
                    </div>
                  </div>

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

          {/* Mode toggle */}
          <div style={{ display: "flex", gap: 6 }}>
            {(["relative", "threshold"] as const).map((m) => (
              <button
                type="button"
                key={m}
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
                  cursor: "pointer",
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
                <div
                  style={{
                    width: 8,
                    height: 5,
                    borderRadius: 3,
                    background: "#78a0b9",
                  }}
                />
                <div
                  style={{
                    width: 8,
                    height: 5,
                    borderRadius: 3,
                    background: "#d4a017",
                  }}
                />
                <div
                  style={{
                    width: 8,
                    height: 5,
                    borderRadius: 3,
                    background: "#c94040",
                  }}
                />
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
      </div>
    </div>
  );
}
