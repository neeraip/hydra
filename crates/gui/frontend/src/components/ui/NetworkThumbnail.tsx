/* WD network thumbnail for the project list. */

interface SvgProps {
  accent: string;
}

// ── Looped distribution grid ──────────────────────────────────────────────────
function Thumbnail({ accent }: SvgProps) {
  const nodes: [number, number][] = [
    [20, 20],
    [60, 15],
    [100, 18],
    [140, 22],
    [30, 45],
    [70, 40],
    [110, 45],
    [150, 42],
    [15, 68],
    [55, 70],
    [95, 65],
    [135, 72],
    [25, 88],
    [75, 85],
    [120, 82],
    [155, 78],
  ];
  const edges: [number, number][] = [
    [0, 1],
    [1, 2],
    [2, 3],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
    [4, 5],
    [5, 6],
    [6, 7],
    [4, 8],
    [5, 9],
    [6, 10],
    [7, 11],
    [8, 9],
    [9, 10],
    [10, 11],
    [8, 12],
    [9, 13],
    [10, 14],
    [11, 15],
    [12, 13],
    [13, 14],
    [14, 15],
  ];
  return (
    <svg
      viewBox="0 0 170 100"
      style={{ width: "100%", height: "100%" }}
      aria-hidden="true"
    >
      {edges.map(([a, b], i) => (
        <line
          key={i}
          x1={nodes[a][0]}
          y1={nodes[a][1]}
          x2={nodes[b][0]}
          y2={nodes[b][1]}
          stroke={accent}
          strokeWidth="1.2"
          strokeOpacity="0.35"
        />
      ))}
      {nodes.map(([x, y], i) => (
        <circle
          key={i}
          cx={x}
          cy={y}
          r="2.5"
          fill={accent}
          fillOpacity="0.55"
        />
      ))}
    </svg>
  );
}

// ── Public API ───────────────────────────────────────────────────────────────

export function NetworkThumbnail({ accent }: { accent: string }) {
  return <Thumbnail accent={accent} />;
}
