/* Cross-section editor — station/elevation plot with bank stations,
   Manning's n by overbank/channel, and ineffective-flow areas. */

import { useEffect, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import type { CrossSection } from "../../hooks";

export function CrossSectionEditor({ accent }: { accent: string }) {
  const { showToast } = useAppState();
  const [crossSections] = useState<CrossSection[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const xs = crossSections.find((c) => c.id === activeId) ?? null;

  return (
    <div style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}>
      {/* Reach / XS list */}
      <div
        style={{
          width: 220,
          borderRight: "1px solid var(--border)",
          overflow: "auto",
          flexShrink: 0,
        }}
      >
        <div
          style={{
            padding: "10px 12px",
            fontSize: 11,
            color: "var(--text-tertiary)",
            textTransform: "uppercase",
            letterSpacing: 0.4,
            borderBottom: "1px solid var(--border)",
          }}
        >
          Whitlock · {crossSections.length} XS
        </div>
        {crossSections.length === 0 && (
          <div
            style={{
              padding: "16px 12px",
              fontSize: 12,
              color: "var(--text-tertiary)",
            }}
          >
            No cross-sections. Load a network to populate.
          </div>
        )}
        {crossSections.map((c) => {
          const active = c.id === activeId;
          return (
            <button
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
                {c.description}
              </div>
            </button>
          );
        })}
        <button
          onClick={() => showToast("Feature coming soon")}
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
          + Insert XS
        </button>
      </div>

      {/* Right pane */}
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          minHeight: 0,
        }}
      >
        {xs === null ? (
          <div
            style={{
              flex: 1,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <span style={{ fontSize: 13, color: "var(--text-tertiary)" }}>
              Select a cross-section from the list.
            </span>
          </div>
        ) : (
          <>
            <XSHeader xs={xs} accent={accent} />
            <div
              style={{
                flex: 1,
                display: "flex",
                overflow: "hidden",
                minHeight: 0,
              }}
            >
              <div style={{ flex: 1, padding: 16 }}>
                <XSChart xs={xs} accent={accent} />
              </div>
              <div
                style={{
                  width: 280,
                  borderLeft: "1px solid var(--border)",
                  overflow: "auto",
                  flexShrink: 0,
                }}
              >
                <XSProperties xs={xs} accent={accent} />
                <XSPointsTable xs={xs} />
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function XSHeader({ xs, accent }: { xs: CrossSection; accent: string }) {
  return (
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
        {xs.id}
      </div>
      <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
        RS{" "}
        <span style={{ color: accent, fontFamily: "var(--font-mono)" }}>
          {xs.riverStation.toFixed(0)}
        </span>{" "}
        · L bank{" "}
        <span
          style={{
            color: "var(--text-secondary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {" "}
          {xs.bankLeft}m
        </span>{" "}
        · R bank{" "}
        <span
          style={{
            color: "var(--text-secondary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {" "}
          {xs.bankRight}m
        </span>
      </div>
      <div
        style={{
          marginLeft: "auto",
          fontSize: 11,
          color: "var(--text-tertiary)",
        }}
      >
        {xs.points.length} pts
      </div>
    </div>
  );
}

function XSChart({ xs, accent }: { xs: CrossSection; accent: string }) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ w: 600, h: 360 });
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
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
    padR = 16,
    padT = 16,
    padB = 32;
  const W = size.w,
    H = size.h;
  const innerW = W - padL - padR,
    innerH = H - padT - padB;

  const stations = xs.points.map((p) => p.station);
  const elevs = xs.points.map((p) => p.elev);
  const sMin = Math.min(...stations),
    sMax = Math.max(...stations);
  const eMin = Math.min(...elevs);
  const eMax = Math.max(...elevs);
  const eRange = eMax - eMin;
  const ePad = eRange * 0.1;
  const yMin = eMin - ePad,
    yMax = eMax + ePad;

  const sx = (s: number) => padL + ((s - sMin) / (sMax - sMin)) * innerW;
  const sy = (e: number) =>
    padT + innerH - ((e - yMin) / (yMax - yMin)) * innerH;

  // Channel infill polygon (between left and right bank stations)
  const lowestInChannel = Math.min(
    ...xs.points
      .filter((p) => p.station >= xs.bankLeft && p.station <= xs.bankRight)
      .map((p) => p.elev),
  );
  const wsElev =
    lowestInChannel +
    0.6 *
      ((xs.points.find((p) => p.station === xs.bankLeft)?.elev ?? 0) -
        lowestInChannel);

  // ground line
  const groundLine = xs.points
    .map((p) => `${sx(p.station).toFixed(2)},${sy(p.elev).toFixed(2)}`)
    .join(" ");
  // ground fill polygon (down to bottom)
  const groundFill = `M ${sx(sMin)} ${sy(yMin)} L ${xs.points.map((p) => `${sx(p.station)} ${sy(p.elev)}`).join(" L ")} L ${sx(sMax)} ${sy(yMin)} Z`;
  // water polygon (between WS line and ground, only inside banks)
  const waterPts = xs.points.filter(
    (p) => p.station >= xs.bankLeft && p.station <= xs.bankRight,
  );

  const yTicks = ticks(yMin, yMax, 5).map((t) => Math.round(t * 10) / 10);
  const xTicks = ticks(sMin, sMax, 6).map((t) => Math.round(t));

  return (
    <div
      ref={wrapRef}
      style={{ width: "100%", height: "100%", position: "relative" }}
    >
      <svg width={W} height={H} style={{ display: "block" }}>
        <defs>
          <linearGradient id={`xs-w-${xs.id}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={accent} stopOpacity={0.35} />
            <stop offset="100%" stopColor={accent} stopOpacity={0.1} />
          </linearGradient>
        </defs>

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

        {/* Ground fill (sub-surface) */}
        <path d={groundFill} fill="var(--bg-card)" stroke="none" />

        {/* Ineffective flow areas (vertical hatch) */}
        {xs.ineffective && (
          <g opacity={0.35}>
            <rect
              x={sx(xs.ineffective.left)}
              y={sy(xs.ineffective.elevation)}
              width={sx(xs.bankLeft) - sx(xs.ineffective.left)}
              height={sy(yMin) - sy(xs.ineffective.elevation)}
              fill={`${accent}33`}
              stroke={accent}
              strokeDasharray="3 3"
              strokeWidth={0.8}
            />
            <rect
              x={sx(xs.bankRight)}
              y={sy(xs.ineffective.elevation)}
              width={sx(xs.ineffective.right) - sx(xs.bankRight)}
              height={sy(yMin) - sy(xs.ineffective.elevation)}
              fill={`${accent}33`}
              stroke={accent}
              strokeDasharray="3 3"
              strokeWidth={0.8}
            />
          </g>
        )}

        {/* Water surface */}
        <polygon
          points={`${sx(xs.bankLeft)},${sy(wsElev)} ${waterPts.map((p) => `${sx(p.station)},${sy(p.elev)}`).join(" ")} ${sx(xs.bankRight)},${sy(wsElev)}`}
          fill={`url(#xs-w-${xs.id})`}
        />
        <line
          x1={sx(xs.bankLeft)}
          x2={sx(xs.bankRight)}
          y1={sy(wsElev)}
          y2={sy(wsElev)}
          stroke={accent}
          strokeWidth={1.4}
        />

        {/* Ground line */}
        <polyline
          points={groundLine}
          stroke="var(--text-secondary)"
          strokeWidth={1.6}
          fill="none"
        />

        {/* Bank station markers */}
        <line
          x1={sx(xs.bankLeft)}
          x2={sx(xs.bankLeft)}
          y1={padT}
          y2={H - padB}
          stroke="#3daf75"
          strokeDasharray="4 3"
          strokeWidth={1}
        />
        <line
          x1={sx(xs.bankRight)}
          x2={sx(xs.bankRight)}
          y1={padT}
          y2={H - padB}
          stroke="#3daf75"
          strokeDasharray="4 3"
          strokeWidth={1}
        />
        <text
          x={sx(xs.bankLeft) + 3}
          y={padT + 12}
          fontSize="10"
          fill="#3daf75"
        >
          L bank
        </text>
        <text
          x={sx(xs.bankRight) + 3}
          y={padT + 12}
          fontSize="10"
          fill="#3daf75"
        >
          R bank
        </text>

        {/* Axes */}
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
            {t.toFixed(1)}
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
          Elevation (m)
        </text>
        <text
          x={padL + innerW / 2}
          y={H - 6}
          fontSize="10"
          fill="var(--text-tertiary)"
          textAnchor="middle"
        >
          Station (m)
        </text>

        {/* Points */}
        {xs.points.map((p, i) => (
          <circle
            key={i}
            cx={sx(p.station)}
            cy={sy(p.elev)}
            r={hoverIdx === i ? 5 : 3}
            fill={hoverIdx === i ? accent : "var(--bg-app)"}
            stroke="var(--text-secondary)"
            strokeWidth={1.2}
            onMouseEnter={() => setHoverIdx(i)}
            onMouseLeave={() => setHoverIdx(null)}
            style={{ cursor: "pointer" }}
          />
        ))}
      </svg>
      {hoverIdx !== null && (
        <div
          style={{
            position: "absolute",
            left: sx(xs.points[hoverIdx].station) + 8,
            top: sy(xs.points[hoverIdx].elev) - 26,
            background: "var(--bg-overlay)",
            border: "1px solid var(--border)",
            padding: "3px 6px",
            borderRadius: 3,
            fontSize: 11,
            color: "var(--text-primary)",
            fontFamily: "var(--font-mono)",
            whiteSpace: "nowrap",
            pointerEvents: "none",
          }}
        >
          {xs.points[hoverIdx].station.toFixed(1)}m,{" "}
          {xs.points[hoverIdx].elev.toFixed(2)}m
        </div>
      )}
    </div>
  );
}

function XSProperties({ xs, accent }: { xs: CrossSection; accent: string }) {
  const Row = ({
    label,
    value,
    mono,
  }: {
    label: string;
    value: string | number;
    mono?: boolean;
  }) => (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        padding: "5px 12px",
        borderBottom: "1px solid var(--border)",
        fontSize: 12,
      }}
    >
      <span style={{ color: "var(--text-tertiary)" }}>{label}</span>
      <span
        style={{
          color: "var(--text-primary)",
          fontFamily: mono ? "var(--font-mono)" : "var(--font-ui)",
        }}
      >
        {value}
      </span>
    </div>
  );
  return (
    <div>
      <SectionLabel>Geometry</SectionLabel>
      <Row label="Reach" value={xs.reach} />
      <Row
        label="River station"
        value={`${xs.riverStation.toFixed(0)} m`}
        mono
      />
      <Row label="L bank station" value={`${xs.bankLeft.toFixed(1)} m`} mono />
      <Row label="R bank station" value={`${xs.bankRight.toFixed(1)} m`} mono />
      <SectionLabel>Manning's n</SectionLabel>
      <Row label="Channel" value={xs.manningChannel.toFixed(3)} mono />
      <Row label="Left overbank" value={xs.manningOverbankL.toFixed(3)} mono />
      <Row label="Right overbank" value={xs.manningOverbankR.toFixed(3)} mono />
      {xs.ineffective && (
        <>
          <SectionLabel accent={accent}>Ineffective flow</SectionLabel>
          <Row
            label="L extent"
            value={`${xs.ineffective.left.toFixed(1)} m`}
            mono
          />
          <Row
            label="R extent"
            value={`${xs.ineffective.right.toFixed(1)} m`}
            mono
          />
          <Row
            label="Trigger"
            value={`${xs.ineffective.elevation.toFixed(2)} m`}
            mono
          />
        </>
      )}
    </div>
  );
}

function XSPointsTable({ xs }: { xs: CrossSection }) {
  return (
    <div>
      <SectionLabel>Station / elevation</SectionLabel>
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
            <th style={{ ...thStyle, textAlign: "right" }}>Station</th>
            <th style={{ ...thStyle, textAlign: "right" }}>Elev</th>
          </tr>
        </thead>
        <tbody>
          {xs.points.map((p, i) => (
            <tr key={i}>
              <td style={{ ...tdStyle, color: "var(--text-tertiary)" }}>
                {i + 1}
              </td>
              <td style={{ ...tdStyle, textAlign: "right" }}>
                {p.station.toFixed(1)}
              </td>
              <td style={{ ...tdStyle, textAlign: "right" }}>
                {p.elev.toFixed(2)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SectionLabel({
  children,
  accent,
}: {
  children: React.ReactNode;
  accent?: string;
}) {
  return (
    <div
      style={{
        padding: "10px 12px",
        fontSize: 10,
        fontWeight: 600,
        color: accent ?? "var(--text-tertiary)",
        textTransform: "uppercase",
        letterSpacing: 0.5,
        borderBottom: "1px solid var(--border)",
        borderTop: "1px solid var(--border)",
        background: "var(--bg-rail)",
      }}
    >
      {children}
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

function ticks(min: number, max: number, count: number) {
  const out: number[] = [];
  for (let i = 0; i <= count; i++) out.push(min + ((max - min) * i) / count);
  return out;
}
