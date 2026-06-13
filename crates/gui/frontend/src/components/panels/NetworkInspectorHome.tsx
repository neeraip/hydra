import { MagnifyingGlassPlusIcon, XMarkIcon } from "@heroicons/react/16/solid";
import { useMemo, useState } from "react";
import type { Link, Node, Pattern } from "../../hooks";
import { useLinks, useNodes, usePatterns } from "../../hooks";

// ── Sort / filter hook ────────────────────────────────────────────────────────

type SortDir = "asc" | "desc";

function useSortedFiltered<T>(
  items: T[],
  query: string,
  searchKeys: (keyof T)[],
): [T[], string | null, SortDir, (col: string) => void] {
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("asc");

  function toggleSort(col: string) {
    if (sortCol === col) setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    else {
      setSortCol(col);
      setSortDir("asc");
    }
  }

  const result = useMemo(() => {
    const q = query.toLowerCase();
    let arr = q
      ? items.filter((item) =>
          searchKeys.some((k) =>
            String((item as Record<string, unknown>)[k as string] ?? "")
              .toLowerCase()
              .includes(q),
          ),
        )
      : items;
    if (sortCol) {
      arr = [...arr].sort((a, b) => {
        const av = (a as Record<string, unknown>)[sortCol] ?? "";
        const bv = (b as Record<string, unknown>)[sortCol] ?? "";
        const cmp = av < bv ? -1 : av > bv ? 1 : 0;
        return sortDir === "asc" ? cmp : -cmp;
      });
    }
    return arr;
  }, [items, query, sortCol, sortDir, searchKeys]);

  return [result, sortCol, sortDir, toggleSort];
}

// ── Shared table styles ───────────────────────────────────────────────────────

const TH: React.CSSProperties = {
  padding: "5px 8px",
  textAlign: "left",
  fontSize: 10,
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
  padding: "4px 8px",
  fontSize: 11,
  borderBottom: "1px solid rgba(255,255,255,0.04)",
  whiteSpace: "nowrap",
  overflow: "hidden",
  textOverflow: "ellipsis",
  maxWidth: 110,
};

function SortIndicator({
  col,
  sortCol,
  sortDir,
}: {
  col: string;
  sortCol: string | null;
  sortDir: SortDir;
}) {
  if (sortCol !== col)
    return <span style={{ opacity: 0.25, marginLeft: 3 }}>↕</span>;
  return (
    <span style={{ marginLeft: 3, color: "var(--accent)" }}>
      {sortDir === "asc" ? "↑" : "↓"}
    </span>
  );
}

// ── Nodes tab ────────────────────────────────────────────────────────────────

const NODE_SEARCH_KEYS: (keyof Node)[] = ["id", "type"];

function NodesTab({
  query,
  nodes,
  onSelect,
  onZoomTo,
  activeId,
}: {
  query: string;
  nodes: Node[];
  onSelect: (id: string) => void;
  onZoomTo?: (id: string) => void;
  activeId?: string | null;
}) {
  const hasResults = nodes.some((n) => n.pressure != null);
  const [rows, sortCol, sortDir, toggleSort] = useSortedFiltered(
    nodes,
    query,
    NODE_SEARCH_KEYS,
  );

  return (
    <div style={{ overflow: "auto", flex: 1 }}>
      <table
        style={{ width: "100%", minWidth: "100%", borderCollapse: "collapse" }}
      >
        <colgroup>
          <col style={{ width: 64 }} />
          <col style={{ width: 64 }} />
          <col style={{ width: 58 }} />
          <col style={{ width: 58 }} />
          {hasResults && <col style={{ width: 48 }} />}
          {onZoomTo && <col style={{ width: 22 }} />}
        </colgroup>
        <thead>
          <tr>
            {(["id", "type", "elevation", "baseDemand"] as const).map((col) => (
              <th key={col} style={TH} onClick={() => toggleSort(col)}>
                {
                  {
                    id: "ID",
                    type: "Type",
                    elevation: "Elev",
                    baseDemand: "Dem",
                  }[col]
                }
                <SortIndicator col={col} sortCol={sortCol} sortDir={sortDir} />
              </th>
            ))}
            {hasResults && (
              <th style={TH} onClick={() => toggleSort("pressure")}>
                P
                <SortIndicator
                  col="pressure"
                  sortCol={sortCol}
                  sortDir={sortDir}
                />
              </th>
            )}
            {onZoomTo && <th style={TH} />}
          </tr>
        </thead>
        <tbody>
          {rows.map((node) => {
            const isActive = node.id === activeId;
            return (
              <tr
                key={node.id}
                onClick={() => onSelect(node.id)}
                style={{
                  cursor: "pointer",
                  background: isActive ? "rgba(79,142,247,0.14)" : undefined,
                  outline: isActive
                    ? "1px solid rgba(79,142,247,0.3)"
                    : undefined,
                  outlineOffset: "-1px",
                }}
                onMouseEnter={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "rgba(255,255,255,0.04)";
                }}
                onMouseLeave={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "transparent";
                }}
              >
                <td
                  style={{
                    ...TD,
                    color: "var(--accent)",
                    fontWeight: 500,
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  {node.id}
                </td>
                <td
                  style={{
                    ...TD,
                    color: "var(--text-secondary)",
                    textTransform: "capitalize",
                  }}
                >
                  {node.type}
                </td>
                <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                  {node.elevation?.toFixed(1) ?? "—"}
                </td>
                <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                  {node.baseDemand?.toFixed(2) ?? "—"}
                </td>
                {hasResults && (
                  <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                    {node.pressure?.toFixed(1) ?? "—"}
                  </td>
                )}
                {onZoomTo && (
                  <td
                    style={{
                      ...TD,
                      padding: "4px 4px 4px 0",
                      textAlign: "right",
                    }}
                  >
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onZoomTo(node.id);
                      }}
                      style={{
                        background: "transparent",
                        border: "none",
                        padding: 2,
                        cursor: "pointer",
                        color: "var(--text-tertiary)",
                        display: "inline-flex",
                        borderRadius: 3,
                        lineHeight: 0,
                      }}
                      onMouseEnter={(e) => {
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--accent)";
                      }}
                      onMouseLeave={(e) => {
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--text-tertiary)";
                      }}
                    >
                      <MagnifyingGlassPlusIcon
                        style={{ width: 11, height: 11 }}
                      />
                    </button>
                  </td>
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && (
        <div
          style={{
            padding: 14,
            fontSize: 11,
            color: "var(--text-tertiary)",
            fontStyle: "italic",
          }}
        >
          No nodes match.
        </div>
      )}
    </div>
  );
}

// ── Links tab ────────────────────────────────────────────────────────────────

// Hydra OUT-file status codes (status_to_f32 in out_writer.rs)
const STATUS_COLOR: Record<number, string> = {
  3: "var(--status-success)", // Open
  2: "var(--status-error)", // Closed
  0: "var(--status-error)", // XHead (pump overloaded)
  1: "var(--status-error)", // TempClosed
  4: "#d4a017", // Active (control valve)
  6: "#d4a017", // XFcv
  7: "#d4a017", // XPressure
};

const STATUS_LABEL: Record<number, string> = {
  3: "Open",
  2: "Closed",
  0: "Closed (XHead)",
  1: "Temp Closed",
  4: "Active",
  6: "Active (XFcv)",
  7: "Active (XPressure)",
};

const LINK_SEARCH_KEYS: (keyof Link)[] = ["id", "type", "fromId", "toId"];

function LinksTab({
  query,
  links,
  onSelect,
  onZoomTo,
  activeId,
}: {
  query: string;
  links: Link[];
  onSelect: (id: string) => void;
  onZoomTo?: (id: string) => void;
  activeId?: string | null;
}) {
  const hasResults = links.some((l) => l.flow != null);
  const [rows, sortCol, sortDir, toggleSort] = useSortedFiltered(
    links,
    query,
    LINK_SEARCH_KEYS,
  );

  return (
    <div style={{ overflow: "auto", flex: 1 }}>
      <table
        style={{ width: "100%", minWidth: "100%", borderCollapse: "collapse" }}
      >
        <colgroup>
          <col style={{ width: 60 }} />
          <col style={{ width: 52 }} />
          <col style={{ width: 36 }} />
          <col style={{ width: 52 }} />
          {hasResults && <col style={{ width: 52 }} />}
          {onZoomTo && <col style={{ width: 22 }} />}
        </colgroup>
        <thead>
          <tr>
            {(["id", "type", "status", "diameter"] as const).map((col) => (
              <th key={col} style={TH} onClick={() => toggleSort(col)}>
                {{ id: "ID", type: "Type", status: "St.", diameter: "Ø" }[col]}
                <SortIndicator col={col} sortCol={sortCol} sortDir={sortDir} />
              </th>
            ))}
            {hasResults && (
              <th style={TH} onClick={() => toggleSort("flow")}>
                Flow
                <SortIndicator col="flow" sortCol={sortCol} sortDir={sortDir} />
              </th>
            )}
            {onZoomTo && <th style={TH} />}
          </tr>
        </thead>
        <tbody>
          {rows.map((link) => {
            const isActive = link.id === activeId;
            return (
              <tr
                key={link.id}
                onClick={() => onSelect(link.id)}
                style={{
                  cursor: "pointer",
                  background: isActive ? "rgba(79,142,247,0.14)" : undefined,
                  outline: isActive
                    ? "1px solid rgba(79,142,247,0.3)"
                    : undefined,
                  outlineOffset: "-1px",
                }}
                onMouseEnter={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "rgba(255,255,255,0.04)";
                }}
                onMouseLeave={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLElement).style.background =
                      "transparent";
                }}
              >
                <td
                  style={{
                    ...TD,
                    color: "var(--accent)",
                    fontWeight: 500,
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  {link.id}
                </td>
                <td
                  style={{
                    ...TD,
                    color: "var(--text-secondary)",
                    textTransform: "capitalize",
                  }}
                >
                  {link.type}
                </td>
                <td style={TD}>
                  {link.status != null ? (
                    <span
                      data-tooltip={STATUS_LABEL[link.status] ?? "Unknown"}
                      style={{
                        display: "inline-block",
                        width: 7,
                        height: 7,
                        borderRadius: "50%",
                        background:
                          STATUS_COLOR[link.status] ?? "var(--text-tertiary)",
                      }}
                    />
                  ) : (
                    <span style={{ color: "var(--text-tertiary)" }}>—</span>
                  )}
                </td>
                <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                  {link.diameter?.toFixed(0) ?? "—"}
                </td>
                {hasResults && (
                  <td style={{ ...TD, fontFamily: "var(--font-mono)" }}>
                    {link.flow != null ? link.flow.toFixed(2) : "—"}
                  </td>
                )}
                {onZoomTo && (
                  <td
                    style={{
                      ...TD,
                      padding: "4px 4px 4px 0",
                      textAlign: "right",
                    }}
                  >
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        onZoomTo(link.id);
                      }}
                      style={{
                        background: "transparent",
                        border: "none",
                        padding: 2,
                        cursor: "pointer",
                        color: "var(--text-tertiary)",
                        display: "inline-flex",
                        borderRadius: 3,
                        lineHeight: 0,
                      }}
                      onMouseEnter={(e) => {
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--accent)";
                      }}
                      onMouseLeave={(e) => {
                        (e.currentTarget as HTMLButtonElement).style.color =
                          "var(--text-tertiary)";
                      }}
                    >
                      <MagnifyingGlassPlusIcon
                        style={{ width: 11, height: 11 }}
                      />
                    </button>
                  </td>
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && (
        <div
          style={{
            padding: 14,
            fontSize: 11,
            color: "var(--text-tertiary)",
            fontStyle: "italic",
          }}
        >
          No links match.
        </div>
      )}
    </div>
  );
}

// ── Patterns tab ──────────────────────────────────────────────────────────────

function PatternsTab({
  patterns,
  onSelect,
}: {
  patterns: Pattern[];
  onSelect?: (id: string) => void;
}) {
  if (patterns.length === 0) {
    return (
      <div
        style={{
          padding: 14,
          color: "var(--text-tertiary)",
          fontSize: 11,
          fontStyle: "italic",
        }}
      >
        No patterns defined in this network.
      </div>
    );
  }

  return (
    <div
      style={{
        padding: "10px 12px",
        display: "flex",
        flexDirection: "column",
        gap: 16,
        overflowY: "auto",
        flex: 1,
      }}
    >
      {patterns.map((pattern) => {
        const max = Math.max(...pattern.multipliers, 1);
        const peak = Math.max(...pattern.multipliers);
        const VW = 220;
        const H = 44;
        const barW = Math.max(1, VW / pattern.multipliers.length - 1);

        return (
          <button
            type="button"
            key={pattern.id}
            onClick={() => onSelect?.(pattern.id)}
            onKeyDown={(e) => {
              if (!onSelect) return;
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelect(pattern.id);
              }
            }}
            style={{
              cursor: onSelect ? "pointer" : undefined,
              border: "none",
              textAlign: "left",
              background: "transparent",
              borderRadius: 4,
              padding: "4px 6px",
              margin: "-4px -6px",
              transition: "background 0.1s",
            }}
            onMouseEnter={(e) => {
              if (onSelect)
                (e.currentTarget as HTMLButtonElement).style.background =
                  "var(--bg-hover, rgba(255,255,255,0.06))";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background = "";
            }}
          >
            <div
              style={{
                fontSize: 11,
                color: "var(--text-secondary)",
                marginBottom: 6,
              }}
            >
              {pattern.id}
            </div>
            <svg
              width="100%"
              height={H}
              viewBox={`0 0 ${VW} ${H}`}
              preserveAspectRatio="none"
              style={{ display: "block" }}
            >
              <title>
                {pattern.id ? `${pattern.id} preview` : "Pattern preview"}
              </title>
              {pattern.multipliers.map((value, i) => {
                const x = i * (barW + 1);
                const bh = (value / max) * H;
                return (
                  <rect
                    key={`${pattern.id}-${x}`}
                    x={x}
                    y={H - bh}
                    width={barW}
                    height={bh}
                    fill="var(--accent)"
                    opacity={0.75}
                  />
                );
              })}
            </svg>
            <div
              style={{
                fontSize: 10,
                color: "var(--text-tertiary)",
                marginTop: 4,
              }}
            >
              {pattern.multipliers.length} step
              {pattern.multipliers.length === 1 ? "" : "s"} · peak{" "}
              {peak.toFixed(2)}
            </div>
          </button>
        );
      })}
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

type HomeTab = "nodes" | "links" | "patterns";

interface Props {
  /** When omitted the close button is hidden (e.g. when rendered inside the rail). */
  onClose?: () => void;
  onSelectNode: (id: string) => void;
  onSelectLink: (id: string) => void;
  /** When provided, pattern cards are clickable and navigate to the editor. */
  onSelectPattern?: (id: string) => void;
  /** Override the internal `useNodes()` call (e.g. pass merged sim-result nodes). */
  nodes?: Node[];
  /** Override the internal `useLinks()` call (e.g. pass merged sim-result links). */
  links?: Link[];
  /** Highlight this node id in the nodes list (e.g. the currently inspected element). */
  activeNodeId?: string | null;
  /** Highlight this link id in the links list (e.g. the currently inspected element). */
  activeLinkId?: string | null;
  /** When provided, each node row shows a zoom icon that triggers this callback. */
  onZoomToNode?: (id: string) => void;
  /** When provided, each link row shows a zoom icon that triggers this callback. */
  onZoomToLink?: (id: string) => void;
  /**
   * When true the panel renders inline (fills its container) rather than as an
   * absolutely-positioned overlay. Use this when hosting inside the secondary rail.
   */
  embedded?: boolean;
}

export function NetworkInspectorHome({
  onClose,
  onSelectNode,
  onSelectLink,
  onSelectPattern,
  nodes: nodesProp,
  links: linksProp,
  activeNodeId,
  activeLinkId,
  onZoomToNode,
  onZoomToLink,
  embedded,
}: Props) {
  const internalNodes = useNodes();
  const internalLinks = useLinks();
  const allNodes = nodesProp ?? internalNodes;
  const allLinks = linksProp ?? internalLinks;
  const patterns = usePatterns();

  const [tab, setTab] = useState<HomeTab>("nodes");
  const [query, setQuery] = useState("");

  const counts: Record<HomeTab, number> = {
    nodes: allNodes.length,
    links: allLinks.length,
    patterns: patterns.length,
  };

  return (
    <div
      className="inspector-panel"
      style={
        embedded
          ? {
              flex: 1,
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              minHeight: 0,
              width: "100%",
            }
          : {
              position: "absolute",
              right: 0,
              top: 0,
              bottom: 0,
              zIndex: 30,
              display: "flex",
              flexDirection: "column",
            }
      }
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "10px 12px",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
        }}
      >
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            Network
          </div>
          <div
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              marginTop: 1,
            }}
          >
            {allNodes.length} nodes · {allLinks.length} links
          </div>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            data-tooltip="Close"
            style={{
              background: "transparent",
              border: "none",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              padding: 4,
              lineHeight: 1,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
      </div>

      {/* Search */}
      <div style={{ padding: "8px 12px 0", flexShrink: 0 }}>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search…"
          style={{
            width: "100%",
            padding: "5px 8px",
            borderRadius: 6,
            border: "1px solid var(--border)",
            background: "rgba(255,255,255,0.04)",
            color: "var(--text-primary)",
            fontSize: 11,
            outline: "none",
            boxSizing: "border-box",
          }}
        />
      </div>

      {/* Tab strip */}
      <div
        style={{
          display: "flex",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
          background: "var(--bg-rail)",
          marginTop: 8,
          overflowX: "auto",
          scrollbarWidth: "none",
        }}
      >
        {(["nodes", "links", "patterns"] as HomeTab[]).map((t) => {
          const active = t === tab;
          return (
            <button
              type="button"
              key={t}
              onClick={() => setTab(t)}
              className={`inspector-tab${active ? " active" : ""}`}
            >
              <span style={{ textTransform: "capitalize" }}>{t}</span>
              <span
                style={{
                  marginLeft: 4,
                  fontSize: 10,
                  padding: "1px 4px",
                  borderRadius: 4,
                  background: active
                    ? "rgba(79,142,247,0.18)"
                    : "var(--bg-card)",
                  color: active ? "var(--accent)" : "var(--text-tertiary)",
                  fontFamily: "var(--font-mono)",
                }}
              >
                {counts[t]}
              </span>
            </button>
          );
        })}
      </div>

      {/* Tab body */}
      {tab === "nodes" && (
        <NodesTab
          query={query}
          nodes={allNodes}
          onSelect={onSelectNode}
          onZoomTo={onZoomToNode}
          activeId={activeNodeId}
        />
      )}
      {tab === "links" && (
        <LinksTab
          query={query}
          links={allLinks}
          onSelect={onSelectLink}
          onZoomTo={onZoomToLink}
          activeId={activeLinkId}
        />
      )}
      {tab === "patterns" && (
        <PatternsTab patterns={patterns} onSelect={onSelectPattern} />
      )}
    </div>
  );
}
