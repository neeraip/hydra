/* Issues / Notifications drawer.
   Right-side slide-out panel that lists preflight + runtime issues with
   severity filters, source filters, and click-to-deep-link navigation.
   Opens via the Status bar issues counter or ⌘⇧M. */

import { ArrowPathIcon } from "@heroicons/react/16/solid";
import {
  ArrowRightIcon,
  ExclamationCircleIcon,
  ExclamationTriangleIcon,
  InformationCircleIcon,
  XMarkIcon,
} from "@heroicons/react/24/outline";
import { useEffect, useMemo, useState } from "react";
import { useAppState, useSimulation } from "../../AppContext";
import {
  countIssues,
  type Issue,
  type IssueSeverity,
  type IssueSource,
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

type Tab = "active" | "dismissed";

export function IssuesPanel() {
  const { issuesPanelOpen, closeIssuesPanel, setProjectView, page, showToast } =
    useAppState();
  const { issues: contextIssues, setIssues: setContextIssues } =
    useSimulation();
  // Mirror context issues into local state so dismiss/restore works without
  // lifting every mutation back up.
  const [issues, setIssues] = useState<Issue[]>(contextIssues);
  // Sync if context issues change (e.g. a new simulation produces new issues).
  useEffect(() => {
    setIssues(contextIssues);
  }, [contextIssues]);
  // Propagate dismiss/restore back to context so StatusBar badge stays current.
  function syncToContext(next: Issue[]) {
    setIssues(next);
    setContextIssues(next);
  }
  const [tab, setTab] = useState<Tab>("active");
  const [activeSeverity, setActiveSeverity] = useState<Set<IssueSeverity>>(
    () => new Set(["error", "warn", "info"]) as Set<IssueSeverity>,
  );
  const [activeSource, setActiveSource] = useState<Set<IssueSource>>(
    () => new Set(Object.keys(SOURCE_LABEL) as IssueSource[]),
  );
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
    return issues
      .filter((i) => i.dismissed === (tab === "dismissed"))
      .filter((i) => activeSeverity.has(i.severity))
      .filter((i) => activeSource.has(i.source))
      .sort((a, b) => sevRank(a.severity) - sevRank(b.severity));
  }, [issues, tab, activeSeverity, activeSource]);

  const counts = countIssues(issues);
  const selected =
    visible.find((i) => i.id === selectedId) ?? visible[0] ?? null;

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
  function dismiss(id: string) {
    syncToContext(
      issues.map((i) => (i.id === id ? { ...i, dismissed: true } : i)),
    );
  }
  function restore(id: string) {
    syncToContext(
      issues.map((i) => (i.id === id ? { ...i, dismissed: false } : i)),
    );
  }
  function deepLink(issue: Issue) {
    if (!issue.link) return;
    if (page !== "project") {
      showToast("Open a project to navigate to this issue", "warn");
      return;
    }
    setProjectView(issue.link.view);
    closeIssuesPanel();
    showToast(`Navigated to ${issue.link.label ?? issue.link.view}`, "info");
  }

  if (!issuesPanelOpen) return null;

  return (
    <>
      {/* Backdrop */}
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
              fontSize: 13,
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

        {/* Tabs */}
        <div
          style={{
            display: "flex",
            borderBottom: "1px solid var(--border)",
            background: "var(--bg-app)",
          }}
        >
          {(["active", "dismissed"] as Tab[]).map((t) => {
            const on = tab === t;
            return (
              <button
                type="button"
                key={t}
                onClick={() => setTab(t)}
                style={{
                  flex: 1,
                  padding: "8px 10px",
                  background: on ? "var(--bg-panel)" : "transparent",
                  color: on ? "var(--text-primary)" : "var(--text-tertiary)",
                  border: "none",
                  borderBottom: on
                    ? "2px solid var(--accent)"
                    : "2px solid transparent",
                  cursor: "pointer",
                  fontSize: 12,
                  textTransform: "capitalize",
                }}
              >
                {t}
              </button>
            );
          })}
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
          {(Object.keys(SEVERITY_META) as IssueSeverity[]).map((s) => {
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
                  fontSize: 11,
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
            gap: 4,
            padding: "6px 12px 8px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          {(Object.keys(SOURCE_LABEL) as IssueSource[]).map((src) => {
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
                  fontSize: 10,
                  cursor: "pointer",
                }}
              >
                {SOURCE_LABEL[src]}
              </button>
            );
          })}
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
                alignItems: "center",
                justifyContent: "center",
                color: "var(--text-tertiary)",
                fontSize: 13,
                padding: 24,
                textAlign: "center",
              }}
            >
              {tab === "active"
                ? "All clear. No issues match the current filters."
                : "No dismissed issues."}
            </div>
          ) : (
            <>
              <div style={{ flex: "1 1 50%", overflowY: "auto" }}>
                {visible.map((issue) => (
                  <IssueRow
                    key={issue.id}
                    issue={issue}
                    selected={issue.id === selected?.id}
                    onSelect={() => setSelectedId(issue.id)}
                    onDismiss={() => dismiss(issue.id)}
                    onRestore={() => restore(issue.id)}
                    showRestore={tab === "dismissed"}
                  />
                ))}
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
  onDismiss,
  onRestore,
  showRestore,
}: {
  issue: Issue;
  selected: boolean;
  onSelect: () => void;
  onDismiss: () => void;
  onRestore: () => void;
  showRestore: boolean;
}) {
  const m = SEVERITY_META[issue.severity];
  return (
    <div
      onClick={onSelect}
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
            fontSize: 12,
            color: "var(--text-primary)",
          }}
        >
          {issue.code && (
            <span
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: 10,
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
            fontSize: 10,
            color: "var(--text-tertiary)",
          }}
        >
          <span>{SOURCE_LABEL[issue.source]}</span>
          <span>·</span>
          <span>{issue.firstSeen}</span>
        </div>
      </div>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          showRestore ? onRestore() : onDismiss();
        }}
        data-tooltip={showRestore ? "Restore" : "Dismiss"}
        style={{
          background: "transparent",
          border: "none",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          fontSize: 11,
          padding: "0 2px",
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {showRestore ? (
          <ArrowPathIcon style={{ width: 14, height: 14 }} />
        ) : (
          <XMarkIcon style={{ width: 14, height: 14 }} />
        )}
      </button>
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
            fontSize: 11,
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
              fontSize: 11,
              color: "var(--text-tertiary)",
            }}
          >
            {issue.code}
          </span>
        )}
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 10, color: "var(--text-tertiary)" }}>
          {SOURCE_LABEL[issue.source]}
        </span>
      </div>
      <div
        style={{
          fontSize: 13,
          color: "var(--text-primary)",
          fontWeight: 500,
          marginBottom: 6,
        }}
      >
        {issue.title}
      </div>
      <div
        style={{
          fontSize: 12,
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
            fontSize: 11,
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
        fontSize: 10,
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
