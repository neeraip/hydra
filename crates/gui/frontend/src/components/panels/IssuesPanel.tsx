/* Issues / Notifications drawer.
   Right-side slide-out panel: a live-health view of the current scenario's
   preflight + runtime issues, with text search, severity/source filters, and
   click-to-locate navigation. Issues are derived from the model and last run,
   so they clear themselves when the underlying condition is fixed — there is
   deliberately no manual "dismiss". Opens via the status-bar issues counter or
   ⌘⇧M. */

import {
  ArrowRightIcon,
  ExclamationCircleIcon,
  ExclamationTriangleIcon,
  InformationCircleIcon,
  MagnifyingGlassIcon,
  XMarkIcon,
} from "@heroicons/react/24/outline";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState, useSimulation } from "../../AppContext";
import { useCanvasSelection } from "../../canvas/selection-context";
import {
  countIssues,
  type Issue,
  type IssueSeverity,
  type IssueSource,
  useLinks,
  useNodes,
} from "../../hooks";

const SEVERITY_META: Record<
  IssueSeverity,
  { label: string; color: string; Icon: typeof ExclamationCircleIcon }
> = {
  error: {
    label: "Error",
    color: "var(--status-error)",
    Icon: ExclamationCircleIcon,
  },
  warn: {
    label: "Warning",
    color: "var(--status-warning)",
    Icon: ExclamationTriangleIcon,
  },
  info: { label: "Info", color: "#4a90d9", Icon: InformationCircleIcon },
};

const SOURCE_LABEL: Record<IssueSource, string> = {
  preflight: "Preflight",
  runtime: "Runtime",
  quality: "Quality",
  data: "Data",
};

const ALL_SEVERITIES: IssueSeverity[] = ["error", "warn", "info"];
const ALL_SOURCES = Object.keys(SOURCE_LABEL) as IssueSource[];

export function IssuesPanel() {
  const {
    issuesPanelOpen,
    closeIssuesPanel,
    setProjectView,
    page,
    showToast,
    activeProjectId,
  } = useAppState();
  const { issues } = useSimulation();
  const { selectNode, selectLink, zoomToNode, zoomToLink } =
    useCanvasSelection();
  const nodes = useNodes();
  const links = useLinks();

  const [activeSeverity, setActiveSeverity] = useState<Set<IssueSeverity>>(
    () => new Set(ALL_SEVERITIES),
  );
  const [activeSource, setActiveSource] = useState<Set<IssueSource>>(
    () => new Set(ALL_SOURCES),
  );
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // ESC closes.
  useEffect(() => {
    if (!issuesPanelOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeIssuesPanel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [issuesPanelOpen, closeIssuesPanel]);

  const visible = useMemo(() => {
    const q = search.trim().toLowerCase();
    return issues
      .filter((i) => activeSeverity.has(i.severity))
      .filter((i) => activeSource.has(i.source))
      .filter((i) => {
        if (!q) return true;
        return (
          i.title.toLowerCase().includes(q) ||
          i.detail.toLowerCase().includes(q) ||
          (i.code?.toLowerCase().includes(q) ?? false)
        );
      })
      .sort((a, b) => sevRank(a.severity) - sevRank(b.severity));
  }, [issues, activeSeverity, activeSource, search]);

  const counts = countIssues(issues);
  const selected =
    visible.find((i) => i.id === selectedId) ?? visible[0] ?? null;
  const filtersActive =
    search.trim() !== "" ||
    activeSeverity.size < ALL_SEVERITIES.length ||
    activeSource.size < ALL_SOURCES.length;

  // Virtualized list: run warnings alone can produce thousands of issues
  // (e.g. negative-pressure per node), and mounting one card each froze the
  // drawer. Rows are single-line (ellipsized) so the estimate is stable;
  // measureElement corrects for font-metric drift.
  const listScrollRef = useRef<HTMLDivElement | null>(null);
  const rowVirtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => listScrollRef.current,
    estimateSize: () => 50,
    overscan: 10,
  });

  function toggleSeverity(s: IssueSeverity) {
    setActiveSeverity((prev) => {
      const next = new Set(prev);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      if (next.size === 0) return prev;
      return next;
    });
  }
  function toggleSource(src: IssueSource) {
    setActiveSource((prev) => {
      const next = new Set(prev);
      if (next.has(src)) next.delete(src);
      else next.add(src);
      if (next.size === 0) return prev;
      return next;
    });
  }
  function resetFilters() {
    setActiveSeverity(new Set(ALL_SEVERITIES));
    setActiveSource(new Set(ALL_SOURCES));
    setSearch("");
  }
  function deepLink(issue: Issue) {
    if (!issue.link) return;
    if (page !== "project") {
      showToast("Open a project to navigate to this issue", "warn");
      return;
    }
    setProjectView(issue.link.view);
    closeIssuesPanel();
    // Select + fly to the element the issue is about, so clicking an issue
    // takes you to the problem instead of just switching tabs. Run warnings
    // carry no element kind, so discriminate node vs link by lookup. Deferred
    // so the canvas view has activated and its map is ready before the fly-to.
    const assetId = issue.link.assetId;
    if (!assetId) return;
    if (nodes.some((n) => n.id === assetId)) {
      selectNode(assetId);
      window.setTimeout(() => zoomToNode(assetId), 220);
    } else if (links.some((l) => l.id === assetId)) {
      selectLink(assetId);
      window.setTimeout(() => zoomToLink(assetId), 220);
    }
  }

  if (!issuesPanelOpen || !activeProjectId) return null;

  return (
    <>
      {/* Backdrop */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: transparent backdrop closes the drawer on pointer interaction. */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: transparent backdrop closes the drawer on pointer interaction. */}
      <div
        onClick={closeIssuesPanel}
        style={{
          position: "fixed",
          inset: 0,
          background: "transparent",
          zIndex: 80,
        }}
      />
      <aside
        role="dialog"
        aria-label="Issues and notifications"
        style={{
          position: "fixed",
          right: 0,
          top: 0,
          bottom: 24,
          width: 460,
          background: "var(--bg-panel)",
          borderLeft: "1px solid var(--border)",
          boxShadow: "var(--shadow-3)",
          display: "flex",
          flexDirection: "column",
          zIndex: 85,
          animation: "slideInRight 200ms ease-out",
          fontFamily: "var(--font-ui)",
          overflow: "hidden",
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: "12px 14px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            gap: 10,
          }}
        >
          <span
            style={{
              fontSize: "var(--text-lg)",
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            Issues & Notifications
          </span>
          <CountChip n={counts.error} color="var(--status-error)" />
          <CountChip n={counts.warn} color="var(--status-warning)" />
          <CountChip n={counts.info} color="#4a90d9" />
          <div style={{ flex: 1 }} />
          <button
            type="button"
            onClick={closeIssuesPanel}
            aria-label="Close"
            style={{
              background: "transparent",
              border: "none",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              padding: 4,
              borderRadius: 4,
            }}
          >
            <XMarkIcon style={{ width: 16, height: 16 }} />
          </button>
        </div>

        {/* Search */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "8px 12px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <MagnifyingGlassIcon
            style={{ width: 14, height: 14, color: "var(--text-tertiary)" }}
          />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search issues, element ids, codes…"
            style={{
              flex: 1,
              minWidth: 0,
              background: "transparent",
              border: "none",
              color: "var(--text-primary)",
              fontSize: "var(--text-md)",
              fontFamily: "var(--font-ui)",
              outline: "none",
            }}
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              aria-label="Clear search"
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-tertiary)",
                cursor: "pointer",
                padding: 2,
                display: "inline-flex",
              }}
            >
              <XMarkIcon style={{ width: 13, height: 13 }} />
            </button>
          )}
        </div>

        {/* Severity filter row */}
        <div
          style={{
            display: "flex",
            gap: 6,
            padding: "8px 12px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          {ALL_SEVERITIES.map((s) => {
            const m = SEVERITY_META[s];
            const on = activeSeverity.has(s);
            return (
              <button
                type="button"
                key={s}
                onClick={() => toggleSeverity(s)}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 5,
                  background: on ? `${m.color}1f` : "transparent",
                  color: on ? m.color : "var(--text-tertiary)",
                  border: `1px solid ${on ? m.color : "var(--border)"}`,
                  borderRadius: 12,
                  padding: "2px 9px",
                  fontSize: "var(--text-sm)",
                  cursor: "pointer",
                }}
              >
                <m.Icon style={{ width: 12, height: 12 }} />
                {m.label}
              </button>
            );
          })}
        </div>

        {/* Source filter row */}
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            alignItems: "center",
            gap: 4,
            padding: "6px 12px 8px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          {ALL_SOURCES.map((src) => {
            const on = activeSource.has(src);
            return (
              <button
                type="button"
                key={src}
                onClick={() => toggleSource(src)}
                style={{
                  background: on ? "var(--bg-card)" : "transparent",
                  color: on ? "var(--text-primary)" : "var(--text-tertiary)",
                  border: "1px solid var(--border)",
                  borderRadius: 10,
                  padding: "1px 8px",
                  fontSize: "var(--text-xs)",
                  cursor: "pointer",
                }}
              >
                {SOURCE_LABEL[src]}
              </button>
            );
          })}
          <div style={{ flex: 1 }} />
          {filtersActive && (
            <button
              type="button"
              onClick={resetFilters}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--accent)",
                fontSize: "var(--text-xs)",
                cursor: "pointer",
                fontFamily: "var(--font-ui)",
              }}
            >
              Clear filters
            </button>
          )}
        </div>

        {/* List + detail (vertical split) */}
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
          }}
        >
          {visible.length === 0 ? (
            <div
              style={{
                flex: 1,
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                justifyContent: "center",
                gap: 10,
                color: "var(--text-tertiary)",
                fontSize: "var(--text-lg)",
                padding: 24,
                textAlign: "center",
              }}
            >
              {counts.total === 0
                ? "All clear. No issues in the current scenario."
                : "No issues match your search and filters."}
              {counts.total > 0 && filtersActive && (
                <button
                  type="button"
                  onClick={resetFilters}
                  style={{
                    background: "transparent",
                    border: "1px solid var(--border)",
                    borderRadius: 5,
                    color: "var(--text-secondary)",
                    fontSize: "var(--text-sm)",
                    padding: "4px 10px",
                    cursor: "pointer",
                    fontFamily: "var(--font-ui)",
                  }}
                >
                  Clear filters
                </button>
              )}
            </div>
          ) : (
            <>
              <div
                ref={listScrollRef}
                style={{ flex: "1 1 50%", overflowY: "auto" }}
              >
                <div
                  style={{
                    height: rowVirtualizer.getTotalSize(),
                    position: "relative",
                  }}
                >
                  {rowVirtualizer.getVirtualItems().map((vi) => {
                    const issue = visible[vi.index];
                    return (
                      <div
                        key={issue.id}
                        ref={rowVirtualizer.measureElement}
                        data-index={vi.index}
                        style={{
                          position: "absolute",
                          top: 0,
                          left: 0,
                          width: "100%",
                          transform: `translateY(${vi.start}px)`,
                        }}
                      >
                        <IssueRow
                          issue={issue}
                          selected={issue.id === selected?.id}
                          onSelect={() => setSelectedId(issue.id)}
                        />
                      </div>
                    );
                  })}
                </div>
              </div>

              {selected && (
                <div
                  style={{
                    flex: "0 0 220px",
                    borderTop: "1px solid var(--border)",
                    background: "var(--bg-app)",
                    padding: 14,
                    overflowY: "auto",
                  }}
                >
                  <DetailPane
                    issue={selected}
                    onDeepLink={() => deepLink(selected)}
                  />
                </div>
              )}
            </>
          )}
        </div>
      </aside>
    </>
  );
}

function IssueRow({
  issue,
  selected,
  onSelect,
}: {
  issue: Issue;
  selected: boolean;
  onSelect: () => void;
}) {
  const m = SEVERITY_META[issue.severity];
  return (
    // biome-ignore lint/a11y/useSemanticElements: virtualized row is absolutely positioned; a native button breaks the measure ref layout.
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
      style={{
        display: "flex",
        gap: 10,
        padding: "9px 12px",
        borderBottom: "1px solid var(--border)",
        background: selected ? "var(--bg-card)" : "transparent",
        borderLeft: selected ? `3px solid ${m.color}` : "3px solid transparent",
        cursor: "pointer",
      }}
    >
      <m.Icon
        style={{
          width: 14,
          height: 14,
          color: m.color,
          flexShrink: 0,
          marginTop: 1,
        }}
      />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: "var(--text-md)",
            color: "var(--text-primary)",
          }}
        >
          {issue.code && (
            <span
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: "var(--text-xs)",
                color: m.color,
                opacity: 0.85,
              }}
            >
              {issue.code}
            </span>
          )}
          <span
            style={{
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {issue.title}
          </span>
        </div>
        <div
          style={{
            display: "flex",
            gap: 8,
            marginTop: 2,
            fontSize: "var(--text-xs)",
            color: "var(--text-tertiary)",
          }}
        >
          <span>{SOURCE_LABEL[issue.source]}</span>
          <span>·</span>
          <span>{issue.firstSeen}</span>
        </div>
      </div>
    </div>
  );
}

function DetailPane({
  issue,
  onDeepLink,
}: {
  issue: Issue;
  onDeepLink: () => void;
}) {
  const m = SEVERITY_META[issue.severity];
  return (
    <>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          marginBottom: 6,
        }}
      >
        <m.Icon style={{ width: 14, height: 14, color: m.color }} />
        <span
          style={{
            fontSize: "var(--text-sm)",
            fontWeight: 600,
            color: m.color,
            textTransform: "uppercase",
            letterSpacing: 0.4,
          }}
        >
          {m.label}
        </span>
        {issue.code && (
          <span
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
            }}
          >
            {issue.code}
          </span>
        )}
        <div style={{ flex: 1 }} />
        <span
          style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}
        >
          {SOURCE_LABEL[issue.source]}
        </span>
      </div>
      <div
        style={{
          fontSize: "var(--text-lg)",
          color: "var(--text-primary)",
          fontWeight: 500,
          marginBottom: 6,
        }}
      >
        {issue.title}
      </div>
      <div
        style={{
          fontSize: "var(--text-md)",
          color: "var(--text-secondary)",
          lineHeight: 1.55,
        }}
      >
        {issue.detail}
      </div>
      {issue.link && (
        <button
          type="button"
          onClick={onDeepLink}
          style={{
            marginTop: 10,
            display: "inline-flex",
            alignItems: "center",
            gap: 5,
            background: "transparent",
            color: "var(--accent)",
            border: "1px solid var(--accent)",
            borderRadius: 5,
            padding: "4px 10px",
            fontSize: "var(--text-sm)",
            cursor: "pointer",
            fontFamily: "var(--font-ui)",
          }}
        >
          {issue.link.label ?? "Open"}
          <ArrowRightIcon style={{ width: 11, height: 11 }} />
        </button>
      )}
    </>
  );
}

function CountChip({ n, color }: { n: number; color: string }) {
  if (n === 0) return null;
  return (
    <span
      style={{
        fontSize: "var(--text-xs)",
        fontFamily: "var(--font-mono)",
        fontWeight: 600,
        background: `${color}1f`,
        color,
        border: `1px solid ${color}55`,
        padding: "1px 6px",
        borderRadius: 9,
        minWidth: 18,
        textAlign: "center",
      }}
    >
      {n}
    </span>
  );
}

function sevRank(s: IssueSeverity): number {
  switch (s) {
    case "error":
      return 0;
    case "warn":
      return 1;
    case "info":
      return 2;
  }
}
