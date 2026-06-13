/* WD pump-curve editor — head/flow scatter+spline with editable points. */

import { useEffect, useMemo, useRef, useState } from "react";
import {
  type CurvePoint,
  createCurve,
  type PumpCurve,
  useCurves,
} from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";

export function CurveEditor({ accent }: { accent: string }) {
  const { bumpNetwork } = useNetworkVersion();
  const curves = useCurves();
  const [activeId, setActiveId] = useState<string | null>(null);
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const [creating, setCreating] = useState(false);
  const [newId, setNewId] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const newIdRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (creating) newIdRef.current?.focus();
  }, [creating]);

  const curve =
    curves.find((c) => c.id === activeId) ??
    (curves.length > 0 ? curves[0] : null);

  async function handleCreate() {
    const trimmed = newId.trim();
    if (!trimmed) {
      setCreateError("ID required");
      return;
    }
    try {
      await createCurve(trimmed);
      bumpNetwork();
      setActiveId(trimmed);
      setCreating(false);
      setNewId("");
      setCreateError(null);
    } catch (err) {
      setCreateError(typeof err === "string" ? err : "Failed to create curve");
    }
  }

  return (
    <div style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}>
      {/* Curve list */}
      <div
        style={{
          width: 220,
          borderRight: "1px solid var(--border)",
          overflow: "auto",
          flexShrink: 0,
        }}
      >
        {curves.map((c) => {
          const active = c.id === (activeId ?? curves[0]?.id);
          return (
            <button
              type="button"
              key={c.id}
              onClick={() => setActiveId(c.id)}
              style={{
                display: "block",
                width: "100%",
                textAlign: "left",
                padding: "10px 12px",
                border: "none",
                background: active ? `${accent}1f` : "transparent",
                borderLeft: active
                  ? `2px solid ${accent}`
                  : "2px solid transparent",
                cursor: "pointer",
                fontFamily: "var(--font-ui)",
                color: active ? "var(--text-primary)" : "var(--text-secondary)",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 500,
                  fontFamily: "var(--font-mono)",
                }}
              >
                {c.id}
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                  marginTop: 2,
                }}
              >
                {c.curveType} · {c.points.length} pts
              </div>
            </button>
          );
        })}
        {creating ? (
          <div
            style={{
              padding: "8px 12px",
              borderBottom: "1px solid var(--border)",
            }}
          >
            <input
              ref={newIdRef}
              value={newId}
              onChange={(e) => {
                setNewId(e.target.value);
                setCreateError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleCreate();
                if (e.key === "Escape") {
                  setCreating(false);
                  setNewId("");
                  setCreateError(null);
                }
              }}
              placeholder="Curve ID…"
              style={{
                width: "100%",
                height: 26,
                background: "var(--bg-input)",
                border: `1px solid ${createError ? "var(--danger)" : "var(--border-focus)"}`,
                borderRadius: 4,
                padding: "0 6px",
                color: "var(--text-primary)",
                fontFamily: "var(--font-mono)",
                fontSize: 12,
                outline: "none",
                boxSizing: "border-box",
              }}
            />
            {createError && (
              <div
                style={{ fontSize: 11, color: "var(--danger)", marginTop: 3 }}
              >
                {createError}
              </div>
            )}
            <div style={{ display: "flex", gap: 4, marginTop: 6 }}>
              <button
                type="button"
                onClick={handleCreate}
                style={{
                  flex: 1,
                  height: 24,
                  fontSize: 11,
                  background: "var(--accent)",
                  color: "#fff",
                  border: "none",
                  borderRadius: 4,
                  cursor: "pointer",
                }}
              >
                Add
              </button>
              <button
                type="button"
                onClick={() => {
                  setCreating(false);
                  setNewId("");
                  setCreateError(null);
                }}
                style={{
                  flex: 1,
                  height: 24,
                  fontSize: 11,
                  background: "var(--bg-hover)",
                  color: "var(--text-secondary)",
                  border: "none",
                  borderRadius: 4,
                  cursor: "pointer",
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setCreating(true)}
            style={{
              width: "100%",
              padding: "10px 12px",
              border: "none",
              background: "transparent",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              fontSize: 12,
              fontFamily: "var(--font-ui)",
              textAlign: "left",
            }}
          >
            + New curve
          </button>
        )}
      </div>

      {/* Right pane */}
      {curve ? (
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
            minHeight: 0,
          }}
        >
          <div
            style={{
              padding: "12px 16px",
              borderBottom: "1px solid var(--border)",
              display: "flex",
              alignItems: "baseline",
              gap: 12,
            }}
          >
            <div
              style={{
                fontSize: 16,
                fontWeight: 600,
                color: "var(--text-primary)",
                fontFamily: "var(--font-mono)",
              }}
            >
              {curve.id}
            </div>
            <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              attached to{" "}
              <span style={{ color: accent, fontFamily: "var(--font-mono)" }}>
                {curve.pumpId}
              </span>
            </div>
            {curve.bep != null && (
              <div
                style={{
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                  marginLeft: "auto",
                }}
              >
                BEP{" "}
                <span style={{ color: "var(--text-secondary)" }}>
                  {curve.bep} L/s
                </span>
              </div>
            )}
          </div>

          <div
            style={{
              flex: 1,
              display: "flex",
              overflow: "hidden",
              minHeight: 0,
            }}
          >
            <div style={{ flex: 1, padding: 16, minWidth: 0 }}>
              <CurveChart
                curve={curve}
                accent={accent}
                hoverIdx={hoverIdx}
                setHoverIdx={setHoverIdx}
              />
            </div>

            <div
              style={{
                width: 280,
                borderLeft: "1px solid var(--border)",
                overflow: "auto",
                flexShrink: 0,
              }}
            >
              <PointsTable
                curve={curve}
                accent={accent}
                hoverIdx={hoverIdx}
                setHoverIdx={setHoverIdx}
              />
              {curve.notes && (
                <div
                  style={{
                    padding: 12,
                    borderTop: "1px solid var(--border)",
                    fontSize: 12,
                    color: "var(--text-tertiary)",
                    lineHeight: 1.5,
                  }}
                >
                  {curve.notes}
                </div>
              )}
            </div>
          </div>
        </div>
      ) : (
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--text-tertiary)",
            fontSize: 13,
          }}
        >
          No pump curves defined. Use "+ New curve" to create one.
        </div>
      )}
    </div>
  );
}

function CurveChart({
  curve,
  accent,
  hoverIdx,
  setHoverIdx,
}: {
  curve: PumpCurve;
  accent: string;
  hoverIdx: number | null;
  setHoverIdx: (n: number | null) => void;
}) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ w: 600, h: 360 });
  useEffect(() => {
    if (!wrapRef.current) return;
    const ro = new ResizeObserver(() => {
      const rect = wrapRef.current?.getBoundingClientRect();
      if (!rect) return;
      setSize({ w: Math.max(320, rect.width), h: Math.max(220, rect.height) });
    });
    ro.observe(wrapRef.current);
    return () => ro.disconnect();
  }, []);

  const padL = 56,
    padR = 24,
    padT = 16,
    padB = 36;
  const W = size.w,
    H = size.h;
  const innerW = W - padL - padR,
    innerH = H - padT - padB;

  const flows = curve.points.map((p) => p.flow);
  const heads = curve.points.map((p) => p.head);
  const fMax = Math.max(...flows, 1);
  const hMax = Math.max(...heads, 1);
  const fNice = niceMax(fMax);
  const hNice = niceMax(hMax);

  const sx = (f: number) => padL + (f / fNice) * innerW;
  const sy = (h: number) => padT + innerH - (h / hNice) * innerH;

  const polyline = curve.points
    .map((p) => `${sx(p.flow).toFixed(2)},${sy(p.head).toFixed(2)}`)
    .join(" ");
  const areaPath = `M ${sx(curve.points[0].flow)} ${sy(0)} L ${curve.points.map((p) => `${sx(p.flow)} ${sy(p.head)}`).join(" L ")} L ${sx(curve.points[curve.points.length - 1].flow)} ${sy(0)} Z`;

  const xTicks = ticks(0, fNice, 5);
  const yTicks = ticks(0, hNice, 5);

  return (
    <div
      ref={wrapRef}
      style={{ width: "100%", height: "100%", position: "relative" }}
    >
      <svg width={W} height={H} style={{ display: "block" }}>
        <title>Curve preview</title>
        <defs>
          <linearGradient id={`pc-${curve.id}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={accent} stopOpacity={0.25} />
            <stop offset="100%" stopColor={accent} stopOpacity={0.02} />
          </linearGradient>
        </defs>

        {/* gridlines */}
        {yTicks.map((t) => (
          <line
            key={`gy-${t}`}
            x1={padL}
            x2={W - padR}
            y1={sy(t)}
            y2={sy(t)}
            stroke="var(--border)"
            strokeDasharray="2 4"
          />
        ))}
        {xTicks.map((t) => (
          <line
            key={`gx-${t}`}
            y1={padT}
            y2={H - padB}
            x1={sx(t)}
            x2={sx(t)}
            stroke="var(--border)"
            strokeDasharray="2 4"
          />
        ))}

        {/* axes */}
        <line
          x1={padL}
          y1={padT}
          x2={padL}
          y2={H - padB}
          stroke="var(--border)"
        />
        <line
          x1={padL}
          y1={H - padB}
          x2={W - padR}
          y2={H - padB}
          stroke="var(--border)"
        />

        {/* labels */}
        {yTicks.map((t) => (
          <text
            key={`yt-${t}`}
            x={padL - 6}
            y={sy(t)}
            fontSize="10"
            fill="var(--text-tertiary)"
            textAnchor="end"
            dominantBaseline="middle"
          >
            {t}
          </text>
        ))}
        {xTicks.map((t) => (
          <text
            key={`xt-${t}`}
            y={H - padB + 14}
            x={sx(t)}
            fontSize="10"
            fill="var(--text-tertiary)"
            textAnchor="middle"
          >
            {t}
          </text>
        ))}
        <text
          x={padL - 40}
          y={padT + innerH / 2}
          fontSize="10"
          fill="var(--text-tertiary)"
          textAnchor="middle"
          transform={`rotate(-90 ${padL - 40} ${padT + innerH / 2})`}
        >
          Head (m)
        </text>
        <text
          x={padL + innerW / 2}
          y={H - 6}
          fontSize="10"
          fill="var(--text-tertiary)"
          textAnchor="middle"
        >
          Flow (L/s)
        </text>

        {/* fill */}
        <path d={areaPath} fill={`url(#pc-${curve.id})`} />

        {/* curve */}
        <polyline
          points={polyline}
          stroke={accent}
          strokeWidth={1.8}
          fill="none"
          strokeLinejoin="round"
        />

        {/* BEP */}
        {curve.bep != null && (
          <line
            x1={sx(curve.bep)}
            x2={sx(curve.bep)}
            y1={padT}
            y2={H - padB}
            stroke={accent}
            strokeWidth={1}
            strokeDasharray="3 3"
            opacity={0.6}
          />
        )}

        {/* points */}
        {curve.points.map((p, i) => {
          const r = hoverIdx === i ? 5 : 3.5;
          return (
            // biome-ignore lint/a11y/noStaticElementInteractions: SVG points only expose hover feedback.
            <circle
              key={`${p.flow}-${p.head}`}
              cx={sx(p.flow)}
              cy={sy(p.head)}
              r={r}
              fill={hoverIdx === i ? accent : "var(--bg-app)"}
              stroke={accent}
              strokeWidth={1.5}
              onMouseEnter={() => setHoverIdx(i)}
              onMouseLeave={() => setHoverIdx(null)}
              style={{ cursor: "pointer" }}
            />
          );
        })}
      </svg>
    </div>
  );
}

function PointsTable({
  curve,
  accent,
  hoverIdx,
  setHoverIdx,
}: {
  curve: PumpCurve;
  accent: string;
  hoverIdx: number | null;
  setHoverIdx: (n: number | null) => void;
}) {
  const total = useMemo(() => curve.points.length, [curve]);
  return (
    <div>
      <div
        style={{
          padding: "10px 12px",
          fontSize: 11,
          fontWeight: 500,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: 0.4,
          borderBottom: "1px solid var(--border)",
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}
      >
        Points <span style={{ color: "var(--text-disabled)" }}>· {total}</span>
      </div>
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: 12,
          fontFamily: "var(--font-mono)",
        }}
      >
        <thead>
          <tr>
            <th style={thStyle}>#</th>
            <th style={{ ...thStyle, textAlign: "right" }}>Flow</th>
            <th style={{ ...thStyle, textAlign: "right" }}>Head</th>
          </tr>
        </thead>
        <tbody>
          {curve.points.map((p: CurvePoint, i: number) => {
            const isHover = hoverIdx === i;
            return (
              <tr
                key={`${p.flow}-${p.head}`}
                onMouseEnter={() => setHoverIdx(i)}
                onMouseLeave={() => setHoverIdx(null)}
                style={{
                  background: isHover ? `${accent}14` : undefined,
                  borderLeft: isHover
                    ? `2px solid ${accent}`
                    : "2px solid transparent",
                  cursor: "pointer",
                }}
              >
                <td style={{ ...tdStyle, color: "var(--text-tertiary)" }}>
                  {i + 1}
                </td>
                <td style={{ ...tdStyle, textAlign: "right" }}>
                  {p.flow.toFixed(1)}
                </td>
                <td
                  style={{
                    ...tdStyle,
                    textAlign: "right",
                    color: isHover ? accent : "var(--text-primary)",
                  }}
                >
                  {p.head.toFixed(1)}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

const thStyle: React.CSSProperties = {
  padding: "6px 10px",
  fontSize: 10,
  fontWeight: 500,
  color: "var(--text-tertiary)",
  borderBottom: "1px solid var(--border)",
  textAlign: "left",
  textTransform: "uppercase",
  letterSpacing: 0.4,
};
const tdStyle: React.CSSProperties = {
  padding: "5px 10px",
  borderBottom: "1px solid var(--border)",
  color: "var(--text-primary)",
};

function niceMax(v: number) {
  const p = 10 ** Math.floor(Math.log10(v));
  const n = v / p;
  let m: number;
  if (n <= 1) m = 1;
  else if (n <= 2) m = 2;
  else if (n <= 5) m = 5;
  else m = 10;
  return m * p;
}
function ticks(min: number, max: number, count: number) {
  const out: number[] = [];
  for (let i = 0; i <= count; i++) out.push(min + ((max - min) * i) / count);
  return out;
}
