import { XMarkIcon } from "@heroicons/react/24/outline";
import { useCallback, useEffect, useRef, useState } from "react";
import { useAppState, useSimulation, useTasks } from "../../AppContext";
import type { Task } from "../../hooks";
import { cancelRunItem, cancelRunQueue, loadResultMeta } from "../../hooks";
import { RunningCard } from "./TaskTray/RunningCard";
import { CompletedRow, FailedRow, QueuedRow } from "./TaskTray/SettledRows";

// Once a task's data reaches its terminal state (100% / Done), keep showing
// it as a running card for this long before swapping to the settled row.
// Purely cosmetic — gives the already-committed 100%/Done frame a chance to
// actually render instead of being replaced in the same paint it appears in.
const HOLD_MS = 700;

// ── Main component ────────────────────────────────────────────────────────────

export function TaskTray() {
  const { closeTaskTray, setProjectView, setPage, setActiveScenarioId } =
    useAppState();
  const { dismissTask, setResultMeta } = useSimulation();
  const ref = useRef<HTMLDivElement>(null);

  const handleViewResults = useCallback(
    (task: Task) => {
      if (!task.projectId) return;
      const { projectId, scenarioId } = task;
      loadResultMeta(projectId, scenarioId).then((meta) => {
        if (meta) setResultMeta(meta);
        setPage("project");
        setProjectView("analysis");
        if (scenarioId !== undefined) setActiveScenarioId(scenarioId);
        closeTaskTray();
      });
    },
    [
      setPage,
      setProjectView,
      setActiveScenarioId,
      closeTaskTray,
      setResultMeta,
    ],
  );

  const tasks = useTasks();

  // Hold tasks in the "running" bucket briefly after they settle, so the
  // RunningCard's 100%/Done frame (already committed to task data by the
  // time status flips) gets a render before being replaced by the settled
  // row. Tracked here — not in the task data itself — so it's independent
  // of whichever backend path (progress event vs. queue reconciliation)
  // actually produced the completion.
  const prevStatusRef = useRef<Map<string, Task["status"]>>(new Map());
  const [heldUntil, setHeldUntil] = useState<Map<string, number>>(new Map());

  useEffect(() => {
    const prevStatus = prevStatusRef.current;
    const now = Date.now();
    const next = new Map(heldUntil);
    let changed = false;
    for (const t of tasks) {
      const was = prevStatus.get(t.id);
      if (
        was === "running" &&
        (t.status === "completed" || t.status === "failed") &&
        !next.has(t.id)
      ) {
        next.set(t.id, now + HOLD_MS);
        changed = true;
      }
    }
    // Drop holds for tasks that are no longer present (dismissed/cleared).
    const liveIds = new Set(tasks.map((t) => t.id));
    for (const id of Array.from(next.keys())) {
      if (!liveIds.has(id)) {
        next.delete(id);
        changed = true;
      }
    }
    prevStatusRef.current = new Map(tasks.map((t) => [t.id, t.status]));
    if (changed) setHeldUntil(next);
  }, [tasks, heldUntil]);

  // Re-check once the soonest hold expires so held tasks fall through to
  // their real settled state without waiting for some other state change.
  useEffect(() => {
    if (heldUntil.size === 0) return;
    const soonest = Math.min(...Array.from(heldUntil.values()));
    const delay = Math.max(0, soonest - Date.now()) + 20;
    const timer = setTimeout(() => {
      setHeldUntil((prev) => {
        const now = Date.now();
        const next = new Map(prev);
        let changed = false;
        for (const [id, until] of prev) {
          if (until <= now) {
            next.delete(id);
            changed = true;
          }
        }
        return changed ? next : prev;
      });
    }, delay);
    return () => clearTimeout(timer);
  }, [heldUntil]);

  const isHeld = useCallback(
    (id: string) => {
      const until = heldUntil.get(id);
      return until != null && Date.now() < until;
    },
    [heldUntil],
  );

  const runningTasks = tasks.filter(
    (t) => t.status === "running" || isHeld(t.id),
  );
  const queuedTasks = tasks.filter((t) => t.status === "queued");
  const settledTasks = tasks.filter(
    (t) => (t.status === "completed" || t.status === "failed") && !isHeld(t.id),
  );
  const completedCount = settledTasks.filter(
    (t) => t.status === "completed",
  ).length;
  const failedCount = settledTasks.filter((t) => t.status === "failed").length;
  const totalActive = runningTasks.length + queuedTasks.length;
  const totalSettled = settledTasks.length;
  const totalAll = tasks.length;

  // Keep a ref so the click handler always sees the latest value without
  // needing to be re-registered every time totalActive changes.
  const totalActiveRef = useRef(totalActive);
  useEffect(() => {
    totalActiveRef.current = totalActive;
  }, [totalActive]);

  // Close on outside click.
  // Use setTimeout(fn, 0) to defer registration until the macrotask *after*
  // the click that opened the tray has finished propagating.  Without the
  // deferral, React flushes state synchronously during the bubble phase, runs
  // this effect, and the same click immediately triggers the close handler.
  // setTimeout defers past the current event so the very first click the
  // listener sees is always a *new* user gesture.
  const runningCount = runningTasks.length;
  useEffect(() => {
    let timerId: ReturnType<typeof setTimeout>;
    function onClickOutside(e: MouseEvent) {
      if (totalActiveRef.current > 0) return;
      // Use composedPath() instead of contains(e.target) — if a React re-render
      // removes the clicked element from the DOM before this handler runs (e.g.
      // the chevron icon swaps on expand), contains() returns false for a now-
      // detached node even though the click was inside the tray. composedPath()
      // captures the dispatch-time path and is unaffected by later DOM mutations.
      if (ref.current && !e.composedPath().includes(ref.current)) {
        closeTaskTray();
      }
    }
    timerId = setTimeout(() => {
      window.addEventListener("click", onClickOutside);
    }, 0);
    return () => {
      clearTimeout(timerId);
      window.removeEventListener("click", onClickOutside);
    };
  }, [closeTaskTray]);

  function handleCancelItem(task: Task) {
    if (task.id.startsWith("queue-")) {
      cancelRunItem(task.id.slice("queue-".length));
    } else if (task.projectId) {
      cancelRunQueue(task.projectId);
    }
  }

  // "Cancel remaining" requests cancellation for active queue runs and cancels
  // queued items for each represented project.
  function handleCancelAll() {
    const projectIds = new Set(
      [...queuedTasks, ...runningTasks]
        .map((t) => t.projectId)
        .filter(Boolean) as string[],
    );
    projectIds.forEach((pid) => {
      cancelRunQueue(pid);
    });
  }

  function clearSettled() {
    settledTasks.forEach((t) => {
      dismissTask(t.id);
    });
  }

  const isEmpty = totalAll === 0;

  // Batch progress fraction — only shown when there are at least 2 total tasks
  // and at least one is not yet settled.
  const showBatchProgress = totalAll >= 2 && totalActive > 0;
  const batchDone = completedCount + failedCount;

  return (
    <div
      ref={ref}
      style={{
        position: "fixed",
        left: "var(--activity-w)",
        bottom: 48,
        width: 380,
        background: "var(--bg-panel)",
        backdropFilter: "blur(20px)",
        border: "1px solid var(--border-hover)",
        borderRadius: 10,
        boxShadow: "var(--shadow-3)",
        zIndex: 100,
        overflow: "hidden",
        animation: "slideInRight 180ms ease-out",
      }}
    >
      {/* ── Header ── */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "10px 14px",
          borderBottom: "1px solid var(--border)",
          gap: 8,
        }}
      >
        <div
          style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}
        >
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: "var(--text-primary)",
              flexShrink: 0,
            }}
          >
            Tasks
          </span>

          {/* Batch progress fraction */}
          {showBatchProgress && (
            <span
              style={{
                fontSize: 12,
                color: "var(--text-tertiary)",
                fontVariantNumeric: "tabular-nums",
              }}
            >
              {batchDone} / {totalAll}
              {runningCount > 0 && (
                <span style={{ color: "var(--accent)", marginLeft: 6 }}>
                  · {runningCount} running
                </span>
              )}
            </span>
          )}

          {/* Quiet counts when idle */}
          {!showBatchProgress && completedCount > 0 && failedCount === 0 && (
            <span style={{ fontSize: 11, color: "var(--status-success)" }}>
              {completedCount} done
            </span>
          )}
          {!showBatchProgress && failedCount > 0 && (
            <span style={{ fontSize: 11, color: "var(--status-error)" }}>
              {failedCount} failed
              {completedCount > 0 ? ` · ${completedCount} done` : ""}
            </span>
          )}
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            flexShrink: 0,
          }}
        >
          {/* Cancel remaining — only when queued items exist */}
          {queuedTasks.length > 0 && (
            <button
              type="button"
              onClick={handleCancelAll}
              style={{
                border: "1px solid var(--border-hover)",
                background: "transparent",
                color: "var(--text-tertiary)",
                cursor: "pointer",
                padding: "2px 8px",
                borderRadius: 5,
                fontSize: 11,
                fontFamily: "var(--font-ui)",
                transition: "border-color var(--t-fast), color var(--t-fast)",
              }}
              onMouseEnter={(e) => {
                const el = e.currentTarget as HTMLButtonElement;
                el.style.borderColor = "var(--status-error)";
                el.style.color = "var(--status-error)";
              }}
              onMouseLeave={(e) => {
                const el = e.currentTarget as HTMLButtonElement;
                el.style.borderColor = "var(--border-hover)";
                el.style.color = "var(--text-tertiary)";
              }}
            >
              Cancel remaining
            </button>
          )}
          {/* Clear settled tasks */}
          {totalSettled > 0 && totalActive === 0 && (
            <button
              type="button"
              onClick={clearSettled}
              style={{
                border: "none",
                background: "transparent",
                color: "var(--text-tertiary)",
                cursor: "pointer",
                padding: "2px 6px",
                borderRadius: 5,
                fontSize: 11,
                fontFamily: "var(--font-ui)",
                transition: "color var(--t-fast)",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-secondary)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-tertiary)";
              }}
            >
              Clear all
            </button>
          )}
          <button
            type="button"
            onClick={closeTaskTray}
            style={{
              border: "none",
              background: "transparent",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              padding: 4,
              borderRadius: 5,
              display: "flex",
              transition: "background var(--t-fast), color var(--t-fast)",
            }}
            onMouseEnter={(e) => {
              const el = e.currentTarget as HTMLButtonElement;
              el.style.background = "var(--nav-hover)";
              el.style.color = "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              const el = e.currentTarget as HTMLButtonElement;
              el.style.background = "transparent";
              el.style.color = "var(--text-tertiary)";
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>
      </div>

      {/* ── Unified list ── */}
      <div style={{ maxHeight: 440, overflowY: "auto" }}>
        {isEmpty ? (
          <div
            style={{
              padding: "32px 16px",
              textAlign: "center",
              color: "var(--text-tertiary)",
              fontSize: 13,
            }}
          >
            No tasks
          </div>
        ) : (
          <>
            {/* 1. Running tasks — full detail, most prominent */}
            {runningTasks.map((task) => (
              <RunningCard
                key={task.id}
                task={task}
                onCancel={
                  !isHeld(task.id) && task.id.startsWith("queue-")
                    ? () => handleCancelItem(task)
                    : undefined
                }
              />
            ))}

            {/* 2. Queued tasks — compact rows with position badges */}
            {queuedTasks.length > 0 && (
              <>
                {/* Section divider only when there's also a running task above */}
                {runningTasks.length > 0 && (
                  <div
                    style={{
                      padding: "5px 14px 4px",
                      fontSize: 10,
                      fontWeight: 700,
                      letterSpacing: "0.06em",
                      textTransform: "uppercase",
                      color: "var(--text-disabled)",
                      background: "var(--bg-panel)",
                      borderBottom: "1px solid var(--border)",
                    }}
                  >
                    Queued: {queuedTasks.length}
                  </div>
                )}
                {queuedTasks.map((task, i) => (
                  <QueuedRow
                    key={task.id}
                    task={task}
                    position={i + 1}
                    onCancel={() => handleCancelItem(task)}
                  />
                ))}
              </>
            )}

            {/* 3. Settled tasks — completed (expandable) then failed */}
            {settledTasks.length > 0 && totalActive > 0 && (
              <div
                style={{
                  padding: "5px 14px 4px",
                  fontSize: 10,
                  fontWeight: 700,
                  letterSpacing: "0.06em",
                  textTransform: "uppercase",
                  color: "var(--text-disabled)",
                  background: "var(--bg-panel)",
                  borderBottom: "1px solid var(--border)",
                }}
              >
                Finished: {settledTasks.length}
              </div>
            )}
            {settledTasks.map((task) =>
              task.status === "completed" ? (
                <CompletedRow
                  key={task.id}
                  task={task}
                  onViewResults={() => handleViewResults(task)}
                  onDismiss={() => dismissTask(task.id)}
                />
              ) : (
                <FailedRow
                  key={task.id}
                  task={task}
                  onDismiss={() => dismissTask(task.id)}
                />
              ),
            )}
          </>
        )}
      </div>
    </div>
  );
}
