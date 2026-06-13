/* Renders an exhibit spec to an SVG. Pure function of (spec, accent).
   Used both inside the modal preview pane and inline in the report. */

import {
  type ExhibitSpec,
  type Link,
  THEMES,
  useLinks,
  useNodes,
} from "../../hooks";

interface Props {
  spec: ExhibitSpec;
  accent: string;
  /** Width of the rendered SVG; height auto-derived from the network bbox. */
  width?: number;
  /** Show paper-style frame, title, caption — for the report. False inside the modal. */
  framed?: boolean;
}

export function MapExhibitPreview({
  spec,
  accent,
  width = 720,
  framed = false,
}: Props) {
  const theme = THEMES[spec.theme];
  const allNodes = useNodes();
  const allLinks = useLinks();
  const nodes = scopeFilterNodes(spec.scope, allNodes);
  const links = scopeFilterLinks(spec.scope, nodes, allLinks);

  // Bounding box of the visible nodes
  const xs = nodes.map((n) => n.x);
  const ys = nodes.map((n) => n.y);
  const pad = 40;
  const minX = Math.min(...xs) - pad,
    maxX = Math.max(...xs) + pad;
  const minY = Math.min(...ys) - pad,
    maxY = Math.max(...ys) + pad;
  const bw = maxX - minX,
    bh = maxY - minY;
  const height = Math.round(bw === 0 ? width : (bh / bw) * width);

  const valueRange = (() => {
    const vs =
      theme.linkValue !== (() => null)
        ? links.map(theme.linkValue).filter((v): v is number => v !== null)
        : nodes.map(theme.nodeValue).filter((v): v is number => v !== null);
    if (vs.length === 0) return { lo: 0, hi: 1 };
    return { lo: Math.min(...vs), hi: Math.max(...vs) };
  })();

  function colorFor(v: number): string {
    const stops = theme.stops;
    let lower = stops[0],
      upper = stops[stops.length - 1];
    for (let i = 0; i < stops.length - 1; i++) {
      if (v >= stops[i].v && v <= stops[i + 1].v) {
        lower = stops[i];
        upper = stops[i + 1];
        break;
      }
    }
    if (v <= stops[0].v) return stops[0].color;
    if (v >= stops[stops.length - 1].v) return stops[stops.length - 1].color;
    const t = (v - lower.v) / Math.max(1e-9, upper.v - lower.v);
    return mix(lower.color, upper.color, t);
  }

  // ── Visuals per style ───────────────────────────────────────────────────
  const isLinkTheme =
    theme.linkValue({
      id: "x",
      type: "pipe",
      fromId: "",
      toId: "",
      velocity: 1,
      diameter: 100,
    } satisfies Link) !== null;

  return (
    <svg
      viewBox={`${minX} ${minY} ${bw} ${bh}`}
      width={width}
      height={height}
      style={{
        background: framed ? "#f8f9fb" : "var(--bg-app)",
        border: framed ? "1px solid #d8dde6" : "1px solid var(--border)",
        borderRadius: framed ? 4 : 6,
        display: "block",
      }}
    >
      <title>{spec.title || "Map exhibit preview"}</title>
      {/* Heatmap underlay (optional) */}
      {spec.style === "heatmap" && (
        <g opacity={0.55}>
          {nodes.map((n) => {
            const v = theme.nodeValue(n) ?? theme.defaultNode;
            return (
              <circle
                key={`h-${n.id}`}
                cx={n.x}
                cy={n.y}
                r={70}
                fill={colorFor(v)}
                style={{ filter: "blur(22px)" }}
              />
            );
          })}
        </g>
      )}

      {/* Links */}
      <g>
        {links.map((l) => {
          const a = nodes.find((n) => n.id === l.fromId);
          const b = nodes.find((n) => n.id === l.toId);
          if (!a || !b) return null;
          const v = theme.linkValue(l);
          let stroke = "#cfd5e0",
            sw = 2;
          if (isLinkTheme && v !== null) {
            stroke = colorFor(v);
            if (spec.style === "graduated") {
              const t =
                (v - valueRange.lo) /
                Math.max(1e-9, valueRange.hi - valueRange.lo);
              sw = 1.5 + t * 7;
            } else {
              sw = 3;
            }
          }
          return (
            <line
              key={l.id}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
              stroke={stroke}
              strokeWidth={sw}
              strokeLinecap="round"
              strokeDasharray={l.type === "pump" ? "4 3" : undefined}
            />
          );
        })}
      </g>

      {/* Nodes */}
      <g>
        {nodes.map((n) => {
          const v = theme.nodeValue(n) ?? theme.defaultNode;
          const baseR = n.type === "reservoir" ? 9 : n.type === "tank" ? 8 : 5;
          let r = baseR;
          let fill = isLinkTheme ? "#1c2230" : colorFor(v);
          if (spec.style === "dot" && !isLinkTheme) {
            const t =
              (v - valueRange.lo) /
              Math.max(1e-9, valueRange.hi - valueRange.lo);
            r = 3 + t * 12;
            fill = colorFor(v);
          }
          return (
            <g key={n.id}>
              {n.type === "reservoir" ? (
                <rect
                  x={n.x - 8}
                  y={n.y - 8}
                  width={16}
                  height={16}
                  fill={fill}
                  stroke="#1c2230"
                  strokeWidth={1.2}
                />
              ) : n.type === "tank" ? (
                <rect
                  x={n.x - 7}
                  y={n.y - 7}
                  width={14}
                  height={14}
                  rx={2}
                  fill={fill}
                  stroke="#1c2230"
                  strokeWidth={1.2}
                />
              ) : (
                <circle
                  cx={n.x}
                  cy={n.y}
                  r={r}
                  fill={fill}
                  stroke="#1c2230"
                  strokeWidth={0.8}
                />
              )}
            </g>
          );
        })}
      </g>

      {/* Callouts */}
      <g>
        {spec.callouts.map((c) => {
          const n = nodes.find((nn) => nn.id === c.nodeId);
          if (!n) return null;
          const lx = n.x + 50,
            ly = n.y - 50;
          return (
            <g key={c.id}>
              <line
                x1={n.x}
                y1={n.y}
                x2={lx}
                y2={ly}
                stroke={accent}
                strokeWidth={1.2}
              />
              <circle
                cx={n.x}
                cy={n.y}
                r={9}
                fill="none"
                stroke={accent}
                strokeWidth={1.5}
              />
              <rect
                x={lx}
                y={ly - 12}
                width={callBoxWidth(c.text)}
                height={20}
                fill="#fff"
                stroke={accent}
                strokeWidth={1}
                rx={3}
              />
              <text
                x={lx + 6}
                y={ly + 2}
                fontSize={10}
                fill="#1a1a1a"
                fontFamily="var(--font-ui)"
              >
                {c.text}
              </text>
            </g>
          );
        })}
      </g>

      {/* North arrow */}
      {spec.showNorth && (
        <g transform={`translate(${maxX - 30} ${minY + 28})`}>
          <circle
            r={14}
            fill="rgba(255,255,255,0.85)"
            stroke="#888"
            strokeWidth={0.8}
          />
          <polygon points="0,-9 4,4 0,1 -4,4" fill="#1c2230" />
          <text
            y={20}
            textAnchor="middle"
            fontSize={9}
            fill="#1a1a1a"
            fontFamily="var(--font-ui)"
          >
            N
          </text>
        </g>
      )}

      {/* Scale bar */}
      {spec.showScale && (
        <g transform={`translate(${minX + 16} ${maxY - 18})`}>
          <rect x={0} y={-3} width={50} height={4} fill="#1c2230" />
          <rect
            x={50}
            y={-3}
            width={50}
            height={4}
            fill="#fff"
            stroke="#1c2230"
            strokeWidth={0.5}
          />
          <text
            x={0}
            y={14}
            fontSize={9}
            fill="#1a1a1a"
            fontFamily="var(--font-ui)"
          >
            0
          </text>
          <text
            x={48}
            y={14}
            fontSize={9}
            fill="#1a1a1a"
            fontFamily="var(--font-ui)"
          >
            50
          </text>
          <text
            x={92}
            y={14}
            fontSize={9}
            fill="#1a1a1a"
            fontFamily="var(--font-ui)"
          >
            100 m
          </text>
        </g>
      )}

      {/* Legend */}
      {spec.showLegend && (
        <g transform={`translate(${minX + 16} ${minY + 18})`}>
          <rect
            x={-6}
            y={-12}
            width={150}
            height={theme.stops.length * 16 + 22}
            fill="rgba(255,255,255,0.92)"
            stroke="#888"
            strokeWidth={0.5}
            rx={2}
          />
          <text
            x={0}
            y={2}
            fontSize={10}
            fontWeight={600}
            fill="#1a1a1a"
            fontFamily="var(--font-ui)"
          >
            {theme.label} ({theme.unit})
          </text>
          {theme.stops.map((s, i) => (
            <g
              key={`${s.v}-${s.color}`}
              transform={`translate(0 ${i * 14 + 14})`}
            >
              <rect
                x={0}
                y={-7}
                width={14}
                height={9}
                fill={s.color}
                stroke="#1c2230"
                strokeWidth={0.4}
              />
              <text
                x={20}
                y={1}
                fontSize={9}
                fill="#1a1a1a"
                fontFamily="var(--font-ui)"
              >
                {s.label ?? `${s.v}`}
              </text>
            </g>
          ))}
        </g>
      )}
    </svg>
  );
}

function scopeFilterNodes(
  scope: ExhibitSpec["scope"],
  all: ReturnType<typeof useNodes>,
) {
  switch (scope) {
    case "selection":
      return all.filter(
        (n) => n.type !== "junction" || all.indexOf(n) % 3 === 0,
      );
    case "south-side":
      return all.filter((n) => n.y >= 280 || n.type !== "junction");
    case "north-feed":
      return all.filter((n) => n.y <= 280 || n.type === "reservoir");
    case "whole":
      return all;
  }
}
function scopeFilterLinks(
  scope: ExhibitSpec["scope"],
  nodes: ReturnType<typeof useNodes>,
  all: ReturnType<typeof useLinks>,
) {
  if (scope === "whole") return all;
  const ids = new Set(nodes.map((n) => n.id));
  return all.filter((l) => ids.has(l.fromId) && ids.has(l.toId));
}

function callBoxWidth(text: string): number {
  return Math.max(80, text.length * 5.6 + 12);
}

function mix(a: string, b: string, t: number): string {
  const ca = parseHex(a),
    cb = parseHex(b);
  const r = Math.round(ca[0] + (cb[0] - ca[0]) * t);
  const g = Math.round(ca[1] + (cb[1] - ca[1]) * t);
  const bl = Math.round(ca[2] + (cb[2] - ca[2]) * t);
  return `rgb(${r}, ${g}, ${bl})`;
}
function parseHex(c: string): [number, number, number] {
  if (c.startsWith("#") && c.length === 7) {
    return [
      parseInt(c.slice(1, 3), 16),
      parseInt(c.slice(3, 5), 16),
      parseInt(c.slice(5, 7), 16),
    ];
  }
  return [120, 120, 120];
}
