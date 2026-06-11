/**
 * Legend — compact colour-scale legend overlay for the canvas.
 *
 * Renders as a small pill chip at the bottom-left of the canvas showing
 * the active node and link variable ramps.  Clicking it opens a popover
 * with full gradient bars, min/max labels, and optional threshold editing
 * when colorMode is "threshold".
 */

import React, { type CSSProperties, useEffect, useState } from "react";
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
  border: "1px solid rgba(255,255,255,0.12)",
  background: "rgba(255,255,255,0.06)",
  color: "var(--text-primary)",
  fontSize: 10,
  boxSizing: "border-box",
};

// ── Legend component ──────────────────────────────────────────────────────────

const SELECT_STYLE: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 600,
  color: "var(--text-primary)",
  background: "rgba(255,255,255,0.06)",
  border: "none",
  outline: "none",
  cursor: "pointer",
  padding: "3px 4px",
  width: "100%",
  fontFamily: "var(--font-ui)",
  borderRadius: 5,
  marginBottom: 5,
};

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
  const [open, setOpen] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [editing, setEditing] = useState(false);

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
      {/* ── Popover ─────────────────────────────────────────────────────── */}
      {open && (
        <div
          style={{
            marginBottom: 8,
            background: "rgba(12,14,18,0.82)",
            backdropFilter: "blur(20px) saturate(160%)",
            WebkitBackdropFilter: "blur(20px) saturate(160%)",
            border: "1px solid rgba(255,255,255,0.10)",
            borderRadius: 10,
            padding: "10px 14px",
            width: 200,
            boxShadow:
              "0 8px 32px rgba(0,0,0,0.65), inset 0 1px 0 rgba(255,255,255,0.06)",
            display: "flex",
            flexDirection: "column",
            gap: 12,
          }}
        >
          {/* Node variable selector + ramp */}
          <div>
            <select
              value={nodeVar}
              onChange={(e) => setNodeVar(e.target.value as NodeVariable)}
              style={SELECT_STYLE}
            >
              <option value="pressure">Pressure (m)</option>
              <option value="head">Head (m)</option>
              <option value="demand">Demand (L/s)</option>
              {qualityMode !== "none" && (
                <option value="quality">Quality</option>
              )}
            </select>
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

          {/* Link variable selector + ramp */}
          <div>
            <select
              value={linkVar}
              onChange={(e) => setLinkVar(e.target.value as LinkVariable)}
              style={SELECT_STYLE}
            >
              <option value="flow">Flow (L/s)</option>
              <option value="velocity">Velocity (m/s)</option>
              <option value="status">Status</option>
            </select>
            {linkVar === "status" ? (
              <StatusSwatches />
            ) : (
              <>
                <Ramp
                  label=""
                  gradient={lg!}
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
                onClick={() => setEditing((v) => !v)}
                style={{
                  width: "100%",
                  padding: "5px 8px",
                  borderRadius: 6,
                  border: "1px solid rgba(255,255,255,0.12)",
                  background: editing
                    ? "rgba(74,144,217,0.22)"
                    : "rgba(255,255,255,0.05)",
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
                key={m}
                onClick={() => {
                  onColorModeChange(m);
                  if (m === "relative") setEditing(false);
                }}
                style={{
                  flex: 1,
                  padding: "5px 8px",
                  borderRadius: 6,
                  border: "1px solid rgba(255,255,255,0.12)",
                  background:
                    colorMode === m
                      ? "rgba(74,144,217,0.22)"
                      : "rgba(255,255,255,0.05)",
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

      {/* ── Compact chip ─────────────────────────────────────────────────── */}
      <button
        onClick={() => setOpen((o) => !o)}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "6px 10px",
          minHeight: 32,
          borderRadius: 20,
          border: `1px solid ${hovered || open ? "rgba(255,255,255,0.20)" : "rgba(255,255,255,0.10)"}`,
          background:
            hovered || open ? "rgba(30,34,42,0.88)" : "rgba(12,14,18,0.75)",
          backdropFilter: "blur(20px) saturate(160%)",
          WebkitBackdropFilter: "blur(20px) saturate(160%)",
          cursor: "pointer",
          boxShadow:
            hovered || open
              ? "0 6px 20px rgba(0,0,0,0.70), inset 0 1px 0 rgba(255,255,255,0.10)"
              : "0 4px 16px rgba(0,0,0,0.55), inset 0 1px 0 rgba(255,255,255,0.06)",
          transition:
            "background 120ms ease, border-color 120ms ease, box-shadow 120ms ease",
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
          {/* Node ramp swatch */}
          <div
            style={{ width: 52, height: 5, borderRadius: 3, background: ng }}
          />
          {/* Link ramp swatch (or discrete status swatches) */}
          {linkVar === "status" ? (
            <div style={{ display: "flex", gap: 2 }}>
              <div
                style={{
                  width: 17,
                  height: 5,
                  borderRadius: 3,
                  background: "#78a0b9",
                }}
              />
              <div
                style={{
                  width: 17,
                  height: 5,
                  borderRadius: 3,
                  background: "#d4a017",
                }}
              />
              <div
                style={{
                  width: 17,
                  height: 5,
                  borderRadius: 3,
                  background: "#c94040",
                }}
              />
            </div>
          ) : (
            <div
              style={{ width: 52, height: 5, borderRadius: 3, background: lg! }}
            />
          )}
        </div>
        <span
          style={{
            fontSize: 10,
            color: "var(--text-secondary)",
            lineHeight: 1.2,
            whiteSpace: "nowrap",
          }}
        >
          Legend
        </span>
      </button>
    </div>
  );
}
