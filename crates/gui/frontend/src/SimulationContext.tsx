/**
 * Simulation context — shared simulation result, task list, run queue, and the
 * derived issues list, so CanvasView, AnalysisView, TaskTray, StatusBar and the
 * IssuesPanel share one source. Extracted from AppContext; SimulationProvider
 * consumes useAppState (a runtime-lazy module cycle, safe because both are only
 * used inside function bodies).
 */

import {
  createContext,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useAppState } from "./AppContext";
import { useCanvasStatus } from "./canvas/status-context";
import {
  compareTopologyDigests,
  fetchRunWarnings,
  fetchValidationFindings,
  getNetworkDigest,
  getPumpEnergy,
  getRunQueue,
  type Issue,
  listenRunQueueUpdate,
  listenSimulationProgress,
  loadResultMeta,
  type PumpEnergyRecord,
  type ResultMeta,
  type RunQueueItem,
  runWarningsToIssues,
  type Task,
  useProjects,
  validationFindingsToIssues,
} from "./hooks";
import { useNetworkVersion } from "./hooks/NetworkVersionContext";
import { backfillTask, taskNeedsBackfill } from "./hooks/taskBackfill";

/** "HH:MM" label used for task and issue timestamps. */
function formatClockTime(date: Date = new Date()): string {
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** "HH:MM" label for a queue item's unix-seconds finish time (now if unset). */
function finishTimeLabel(finishedAt: number | null): string {
  return formatClockTime(
    finishedAt != null ? new Date(finishedAt * 1000) : new Date(),
  );
}

// ── Simulation state ───────────────────────────────────────────────────────
//
// Shared simulation result, task list, and issues list so CanvasView,
// AnalysisView, TaskTray, StatusBar, IssuesPanel etc. share one source.

interface SimulationCtxValue {
  /** Pump energy loaded from the results file epilog (tiny; safe to hold in memory). */
  pumpEnergy: PumpEnergyRecord[] | null;
  setPumpEnergy: (energy: PumpEnergyRecord[] | null) => void;
  /** Global times + ranges loaded from results.out header/epilog. */
  resultMeta: ResultMeta | null;
  /** True while loading result metadata for the active project/scenario. */
  resultMetaLoading: boolean;
  setResultMeta: (meta: ResultMeta | null) => void;
  /**
   * Opaque freshness token for `resultMeta`: incremented every time result
   * metadata is (re)loaded from disk via `loadResultMeta` — on project/
   * scenario switch and again when a run completes. Consumers caching
   * derived or per-period data keyed on result identity include this in
   * their keys so a re-run that produces value-equal metadata still
   * invalidates the cache.
   */
  resultGeneration: number;
  /**
   * True when the loaded results' topology digest and the live model's digest
   * are BOTH known and differ — i.e. the network's node/link structure changed
   * (including unsaved edits) since the results were produced, so any
   * index-addressed read of them would attach values to the wrong elements.
   * Always false when either digest is unknown (pre-digest `.out` files keep
   * today's ungated behaviour).
   */
  resultsTopologyStale: boolean;
  /**
   * Topology digest (16 hex chars) of the CURRENT model — including unsaved
   * in-memory edits — or `null` while unknown. Consumers holding *other*
   * result metadata (e.g. the comparison baseline's) compare against this via
   * `compareTopologyDigests` to apply the same staleness gating.
   */
  liveNetworkDigest: string | null;
  tasks: Task[];
  issues: Issue[];
  setIssues: Dispatch<SetStateAction<Issue[]>>;
  /** Remove a completed or failed task from the tray. */
  dismissTask: (id: string) => void;
}

const SimCtx = createContext<SimulationCtxValue>({
  pumpEnergy: null,
  setPumpEnergy: () => {},
  resultMeta: null,
  resultMetaLoading: false,
  setResultMeta: () => {},
  resultGeneration: 0,
  resultsTopologyStale: false,
  liveNetworkDigest: null,
  tasks: [],
  issues: [],
  setIssues: () => {},
  dismissTask: () => {},
});

export function SimulationProvider({ children }: { children: ReactNode }) {
  const {
    bumpProjects,
    bumpScenarios,
    projectsVersion,
    activeProjectId,
    activeScenarioId,
    networkLoadFailure,
  } = useAppState();
  const {
    clearEdited,
    editedScenarioIds,
    version: networkVersion,
  } = useNetworkVersion();
  const { coordStatus, coordMissingCount, coordTotalCount } = useCanvasStatus();
  const [pumpEnergy, setPumpEnergy] = useState<PumpEnergyRecord[] | null>(null);
  const [resultMeta, setResultMeta] = useState<ResultMeta | null>(null);
  const [resultMetaLoading, setResultMetaLoading] = useState(false);
  // Incremented on every completed `loadResultMeta` whose result is
  // committed via setResultMeta (the simple, consistent rule — consumers
  // treat it as an opaque freshness token; see SimulationCtxValue).
  const [resultGeneration, setResultGeneration] = useState(0);
  // Topology digest of the CURRENT model (16 hex chars), fetched from the
  // backend; null = unknown (no project, command unavailable, fetch pending).
  const [liveNetworkDigest, setLiveNetworkDigest] = useState<string | null>(
    null,
  );
  const [tasks, setTasks] = useState<Task[]>([]);
  const [issues, setIssues] = useState<Issue[]>([]);
  // Backend `validate_network` findings, already mapped to Issue shape.
  const [validationIssues, setValidationIssues] = useState<Issue[]>([]);
  // Last run's solver warnings (`get_run_warnings`), mapped to Issue shape.
  const [runWarningIssues, setRunWarningIssues] = useState<Issue[]>([]);

  // Fetch validation findings whenever the active project/scenario changes or
  // the network structurally changes (`networkVersion` is the same retrigger
  // the version-keyed data hooks use). Command-missing/error resolves to [].
  // biome-ignore lint/correctness/useExhaustiveDependencies: `networkVersion` is an intentional retrigger — refetch validation after structural network changes.
  useEffect(() => {
    if (!activeProjectId) {
      setValidationIssues([]);
      return;
    }
    let cancelled = false;
    const firstSeen = formatClockTime();
    fetchValidationFindings(activeProjectId, activeScenarioId).then(
      (findings) => {
        if (cancelled) return;
        setValidationIssues(validationFindingsToIssues(findings, firstSeen));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, activeScenarioId, networkVersion]);

  // Refresh the live model's topology digest whenever the target changes,
  // the network structurally changes (`networkVersion` bumps on create/
  // delete — the digest is invariant to property edits, so a refetch after
  // one is a cheap no-op), or fresh results land (`resultGeneration` bumps —
  // a run may follow a save, and comparing a fresh result digest against a
  // stale live digest would misreport). Failure/unavailable resolves to null
  // = unknown → no gating.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `networkVersion` and `resultGeneration` are intentional retriggers — see comment above.
  useEffect(() => {
    if (!activeProjectId) {
      setLiveNetworkDigest(null);
      return;
    }
    let cancelled = false;
    getNetworkDigest(activeProjectId, activeScenarioId).then((digest) => {
      if (!cancelled) setLiveNetworkDigest(digest);
    });
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, activeScenarioId, networkVersion, resultGeneration]);

  // Stale exactly when both digests are known and differ; unknown (either
  // side missing) keeps today's ungated behaviour for pre-digest results.
  const resultsTopologyStale =
    compareTopologyDigests(resultMeta?.networkDigest, liveNetworkDigest) ===
    "stale";

  // Fetch the last run's solver warnings whenever the active project/scenario
  // changes or fresh results land (`resultGeneration` bumps on every
  // (re)loaded result metadata). Command-missing/error/no-run resolves to [].
  // biome-ignore lint/correctness/useExhaustiveDependencies: `resultGeneration` is an intentional retrigger — refetch run warnings after every completed run.
  useEffect(() => {
    if (!activeProjectId) {
      setRunWarningIssues([]);
      return;
    }
    let cancelled = false;
    const firstSeen = formatClockTime();
    fetchRunWarnings(activeProjectId, activeScenarioId).then((warnings) => {
      if (cancelled) return;
      setRunWarningIssues(runWarningsToIssues(warnings, firstSeen));
    });
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, activeScenarioId, resultGeneration]);

  // Derive live issues from runtime/task/network signals. This keeps the
  // Issues drawer populated without requiring manual seeding.
  useEffect(() => {
    if (!activeProjectId) {
      setIssues([]);
      return;
    }

    const firstSeenNow = formatClockTime();

    const next: Issue[] = [];
    // All derived issues share the same canvas link and freshness fields.
    const pushIssue = (issue: Omit<Issue, "link" | "firstSeen">) => {
      next.push({
        ...issue,
        link: { view: "canvas", label: "Open canvas" },
        firstSeen: firstSeenNow,
      });
    };

    const runningForProject = tasks.filter(
      (t) => t.projectId === activeProjectId && t.status === "running",
    );
    const queuedForProject = tasks.filter(
      (t) => t.projectId === activeProjectId && t.status === "queued",
    );

    if (runningForProject.length > 0) {
      pushIssue({
        id: `runtime-running-${activeProjectId}`,
        severity: "info",
        source: "runtime",
        code: "SIM-RUNNING",
        title:
          runningForProject.length === 1
            ? "Simulation in progress"
            : `${runningForProject.length} simulations in progress`,
        detail:
          "Hydraulics/quality solve is currently running. Results and status badges will update automatically when complete.",
      });
    }

    if (queuedForProject.length > 0) {
      pushIssue({
        id: `runtime-queued-${activeProjectId}`,
        severity: "info",
        source: "runtime",
        code: "SIM-QUEUED",
        title:
          queuedForProject.length === 1
            ? "Simulation queued"
            : `${queuedForProject.length} simulations queued`,
        detail:
          "One or more runs are queued and will execute when backend workers are available.",
      });
    }

    if (
      !resultMeta &&
      runningForProject.length === 0 &&
      queuedForProject.length === 0
    ) {
      pushIssue({
        id: `preflight-no-results-${activeScenarioId ?? "base"}`,
        severity: "info",
        source: "preflight",
        code: "NO-RESULTS",
        title: "No simulation results for active scenario",
        detail:
          "Run a simulation to populate timeline, analysis summaries, and result overlays for this scenario.",
      });
    }

    if (coordTotalCount > 0 && coordStatus === "empty") {
      pushIssue({
        id: "data-coords-empty",
        severity: "error",
        source: "data",
        code: "COORDS-EMPTY",
        title: "No geospatial coordinates available",
        detail:
          "All nodes are missing geographic coordinates. Map mode cannot place the network until coordinates are provided or corrected.",
      });
    } else if (coordTotalCount > 0 && coordStatus === "partial") {
      pushIssue({
        id: "data-coords-partial",
        severity: "warn",
        source: "data",
        code: "COORDS-PARTIAL",
        title: "Some nodes are missing coordinates",
        detail: `${coordMissingCount} of ${coordTotalCount} nodes are missing map coordinates. Geographic view may be incomplete.`,
      });
    }

    // The model failed to load — most commonly an INP hand-edited outside
    // Hydra that no longer parses. The parser error is line-anchored;
    // persists (unlike the toast) until the next successful load clears it.
    if (networkLoadFailure) {
      pushIssue({
        id: "model-load-failed",
        severity: "error",
        source: "data",
        code: "MODEL-LOAD-FAILED",
        title: "Model failed to load",
        detail: `${networkLoadFailure} — the INP may have been edited outside Hydra. Fix the reported line (Open folder shows the file), or re-import the model.`,
      });
    }

    // Topology drift: results exist but their node/link structure no longer
    // matches the live model, so result overlays/series are gated off until
    // a re-run. Stable id — the issue disappears when the digests match
    // again (re-run, or the edit is undone).
    if (resultsTopologyStale) {
      pushIssue({
        id: "results-topology-stale",
        severity: "warn",
        source: "data",
        code: "RESULTS-TOPOLOGY-STALE",
        title: "Results predate the current network topology",
        detail:
          "Nodes or links were added, removed, or renamed since the loaded results were produced, so result overlays and time series are hidden. Re-run the simulation to refresh them.",
      });
    }

    if (editedScenarioIds.has(activeScenarioId ?? null)) {
      pushIssue({
        id: `preflight-stale-${activeScenarioId ?? "base"}`,
        severity: "warn",
        source: "preflight",
        code: "RESULTS-STALE",
        title: "Network changed since the last run",
        detail:
          "Simulation results may be stale for the active scenario because the network was edited after the last successful run.",
      });
    }

    for (const t of tasks) {
      if (t.status !== "failed") continue;
      if (t.projectId !== activeProjectId) continue;
      pushIssue({
        id: `runtime-task-failed-${t.id}`,
        severity: "error",
        source: "runtime",
        code: "SIM-RUN-FAILED",
        title: `Simulation failed: ${t.scenarioName}`,
        detail: t.errorMessage ?? "Simulation failed.",
      });
    }

    // Backend validation findings (already Issue-shaped; ids are stable
    // code+elementId keys so the first-seen merge below survives re-derivation).
    next.push(...validationIssues);

    // Solver warnings from the last run (also Issue-shaped with stable
    // simwarn-<code>-<elementId|network> ids for the first-seen merge).
    next.push(...runWarningIssues);

    setIssues((prev) => {
      const prevById = new Map(prev.map((i) => [i.id, i]));
      return next.map((i) => {
        const existing = prevById.get(i.id);
        // Preserve the original first-seen time so a still-present issue keeps
        // its age across re-derivations instead of resetting on every refresh.
        return existing ? { ...i, firstSeen: existing.firstSeen } : i;
      });
    });
  }, [
    activeProjectId,
    activeScenarioId,
    coordMissingCount,
    coordStatus,
    coordTotalCount,
    editedScenarioIds,
    networkLoadFailure,
    resultMeta,
    resultsTopologyStale,
    runWarningIssues,
    tasks,
    validationIssues,
  ]);

  // When the active *project* changes, immediately clear stale metadata so we
  // never show one project's result ranges while a different project is loading.
  // Scenario-only switches do NOT clear here — keeping the stale metadata
  // prevents transient nulls from causing deck.gl layer re-initialisation,
  // inspector card unmounts, and the timeline-height CSS variable flip.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `activeProjectId` is an intentional trigger to clear stale result metadata on project switch.
  useEffect(() => {
    setResultMeta(null);
    setPumpEnergy(null);
  }, [activeProjectId]);

  // When the active project OR scenario changes, reload result metadata and
  // pump energy from disk.  Per-period data and cross-period analytics are
  // fetched on-demand by individual views.
  useEffect(() => {
    if (!activeProjectId) {
      setResultMetaLoading(false);
      return;
    }
    let cancelled = false;
    setResultMetaLoading(true);
    loadResultMeta(activeProjectId, activeScenarioId)
      .then((meta) => {
        if (!cancelled) {
          setResultMeta(meta);
          setResultGeneration((g) => g + 1);
        }
      })
      .finally(() => {
        if (!cancelled) setResultMetaLoading(false);
      });
    getPumpEnergy(activeProjectId, activeScenarioId).then((energy) => {
      if (!cancelled) setPumpEnergy(energy);
    });
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, activeScenarioId]);

  // Keep a live ref to the project list so the queue event handler can resolve
  // project names without being captured in a stale closure.
  const projects = useProjects(projectsVersion);
  const projectsRef = useRef(projects);
  useEffect(() => {
    projectsRef.current = projects;
  }, [projects]);

  // Live refs for the active project/scenario so event handlers (which are
  // registered once with no deps) always see the current values.
  const activeProjectIdRef = useRef(activeProjectId);
  const activeScenarioIdRef = useRef(activeScenarioId);
  useEffect(() => {
    activeProjectIdRef.current = activeProjectId;
  }, [activeProjectId]);
  useEffect(() => {
    activeScenarioIdRef.current = activeScenarioId;
  }, [activeScenarioId]);

  /**
   * Reload the loaded-result metadata and pump energy for one target.
   *
   * Shared by the two independent ways a finished run becomes observable —
   * the terminal `simulation_progress` event and the `run_queue_update`
   * snapshot — because either can be the only one that arrives. A fast run
   * can complete before the progress listener sees its terminal event, which
   * left the legend and timeline hidden until the project was reopened.
   *
   * The target is re-checked when the fetch resolves rather than guarded by a
   * cancellation flag: the risk is not a stale render but applying one
   * scenario's results while a different one is on screen.
   */
  /** Run ids whose completion has already refreshed results. `run_queue_update`
   * re-sends the whole queue on every change, so a finished run appears in
   * many snapshots; without this each one would refetch. */
  const refreshedRunsRef = useRef<Set<string>>(new Set());

  const refreshResultsFor = useCallback((pid: string, sid: string | null) => {
    void loadResultMeta(pid, sid).then((meta) => {
      if (!meta) return;
      if (activeProjectIdRef.current !== pid) return;
      if (activeScenarioIdRef.current !== sid) return;
      setResultMeta(meta);
      setResultGeneration((g) => g + 1);
    });
    void getPumpEnergy(pid, sid).then((energy) => {
      if (!energy) return;
      if (activeProjectIdRef.current !== pid) return;
      if (activeScenarioIdRef.current !== sid) return;
      setPumpEnergy(energy);
    });
  }, []);

  // Subscribe to backend simulation_progress events and pipe them into the
  // running task so the TaskTray shows live %, phase label, and progress bar.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listenSimulationProgress((ev) => {
      setTasks((prev) => {
        // Every run is queued, so the run id always identifies its task.
        // (There used to be a fallback to "the first running task" for
        // non-queued runs; with two runs in flight it attributed one run's
        // progress to the other's row.)
        if (ev.runId == null) return prev;
        let tasks = prev;
        let idx = prev.findIndex((t) => t.id === `queue-${ev.runId}`);

        // Timing-race recovery: progress arrived before run_queue_update
        // created the task entry. Synthesise a placeholder immediately so
        // no progress events are dropped.
        //
        // React's functional-update contract guarantees each updater receives
        // the committed output of all previously-enqueued updaters, so a
        // second rapid-fire event will already see the placeholder in `prev`.
        // The explicit `prev.some()` guard below makes this invariant visible
        // and keeps it safe against future refactors of the `idx` search logic.
        if (
          !ev.done &&
          !ev.failed &&
          !prev.some((t) => t.id === `queue-${ev.runId}`)
        ) {
          const placeholder: Task = {
            id: `queue-${ev.runId}`,
            projectName: "…",
            scenarioName: "…",
            status: "running",
            timeLabel: "Running…",
            history: [
              {
                at: Date.now(),
                label:
                  ev.phase === "quality"
                    ? "Phase: Water quality"
                    : "Phase: Hydraulics",
              },
            ],
          };
          tasks = [placeholder, ...prev];
          idx = 0;
        }

        if (idx === -1) return prev;
        const task = tasks[idx];
        const now = Date.now();

        // Build a history entry when the phase changes or on terminal events.
        const prevPhase = task.phase;
        const phaseChanged = ev.phase !== prevPhase && prevPhase !== undefined;
        const newEntries: { at: number; label: string }[] = [];
        if (phaseChanged || (prevPhase === undefined && ev.phase)) {
          newEntries.push({
            at: now,
            label:
              ev.phase === "quality"
                ? "Phase: Water quality"
                : "Phase: Hydraulics",
          });
        }
        if (ev.message && ev.message !== task.progressMessage) {
          newEntries.push({ at: now, label: ev.message });
        }
        if (ev.done) newEntries.push({ at: now, label: "Completed" });
        if (ev.failed) newEntries.push({ at: now, label: "Failed" });

        // Deduplicate adjacent identical labels and cap at 24 entries.
        const prevHistory = task.history ?? [];
        const history = [...prevHistory];
        for (const entry of newEntries) {
          if (
            history.length === 0 ||
            history[history.length - 1].label !== entry.label
          ) {
            history.push(entry);
          }
        }
        const capped = history.slice(-24);

        // Per-phase progress tracking.
        let hydraulicsPercent = task.hydraulicsPercent;
        let hydraulicsDone = task.hydraulicsDone;
        let qualityPercent = task.qualityPercent;
        let qualityDone = task.qualityDone;
        // hasQuality is set from the first event's runQuality flag so the
        // quality "Waiting" bar can appear even before the quality phase starts.
        const hasQuality = task.hasQuality ?? ev.runQuality;

        if (ev.phase === "hydraulics") {
          hydraulicsPercent = ev.percent;
          if (ev.done) {
            hydraulicsPercent = 100;
            // hydraulicsDone intentionally not flipped here — see the
            // staged transition below. Flipping it in this same update
            // would replace the "100%" text with "Done" before the 100%
            // frame ever renders.
          }
        } else if (ev.phase === "quality") {
          // First quality event: mark hydraulics as fully done.
          if (!hydraulicsDone) {
            hydraulicsDone = true;
            hydraulicsPercent = 100;
          }
          qualityPercent = ev.percent;
          if (ev.done) {
            qualityDone = true;
            qualityPercent = 100;
          }
        }

        // Overall ring percent — increases monotonically across both phases.
        const overallPercent = hasQuality
          ? ev.phase === "hydraulics"
            ? ev.percent * 0.5
            : 50 + ev.percent * 0.5
          : ev.percent;

        const next = [...tasks];
        next[idx] = {
          ...task,
          phase: ev.phase as "hydraulics" | "quality",
          progressPercent: overallPercent,
          progressMessage: ev.message ?? undefined,
          simulatedSeconds: ev.simulatedSeconds,
          durationSeconds: ev.durationSeconds,
          history: capped,
          hasQuality,
          hydraulicsPercent,
          hydraulicsDone,
          qualityPercent,
          qualityDone,
          ...(ev.done
            ? {
                status: "completed" as const,
                timeLabel: `Completed ${formatClockTime()}`,
                primaryAction: "View results" as const,
              }
            : {}),
          ...(ev.failed
            ? {
                status: "failed" as const,
                errorMessage: ev.message ?? "Simulation failed",
              }
            : {}),
        };
        return next;
      });

      // NOTE: do NOT bump projects/scenarios here. The backend emits
      // simulation_progress(done=true) BEFORE it writes the final "simulated"
      // state to the DB. Bumping here would refetch while the DB still shows
      // "running". The run_queue_update event fires after the DB commit and is
      // the correct place to trigger UI refreshes.
      //
      // However, the .out file IS fully written by the time done=true fires,
      // so we can safely reload result metadata from disk here. This ensures
      // AnalysisView and other views update immediately on completion rather
      // than waiting for the user to navigate away and back.
      if ((ev.done || ev.failed) && activeProjectIdRef.current) {
        refreshResultsFor(
          activeProjectIdRef.current,
          activeScenarioIdRef.current,
        );
      }
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((e) => {
        // eslint-disable-next-line no-console
        console.error("[sim-progress] failed to register listener:", e);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refreshResultsFor]);

  // Subscribe to run_queue_update events emitted by the backend queue processor.
  // When the queue for the active project changes, fetch the latest items and
  // merge them into the tasks list so the TaskTray stays in sync.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listenRunQueueUpdate((projectId) => {
      // run_queue_update always fires after the backend has committed the
      // new run state (running → done/failed) to the DB, so this is the
      // correct place to refresh project and scenario state badges.
      bumpProjects();
      bumpScenarios();
      getRunQueue(projectId).then((items: RunQueueItem[]) => {
        if (cancelled) return;
        // Clear the stale-results flag for every scenario that just completed
        // successfully.
        for (const item of items) {
          if (item.status === "done") clearEdited(item.targetId);
        }

        // The queue is the authority on completion, and for a fast run its
        // snapshot may be the only place the finish is ever observed — the
        // task-merge below already compensates for a missed terminal progress
        // event, but the results themselves were left unrefreshed, so the
        // legend and timeline stayed hidden until the project was reopened.
        const activePid = activeProjectIdRef.current;
        const activeSid = activeScenarioIdRef.current;
        if (activePid !== null && projectId === activePid) {
          const finished = items.filter(
            (i) =>
              i.status === "done" &&
              i.targetId === activeSid &&
              !refreshedRunsRef.current.has(i.id),
          );
          if (finished.length > 0) {
            for (const i of finished) refreshedRunsRef.current.add(i.id);
            refreshResultsFor(activePid, activeSid);
          }
        }
        setTasks((prev) => {
          // Only (re)create task entries for items that are actively queued or
          // running. Progress events own the completed/failed transitions, so
          // we must not overwrite those — that would cause "merging" where
          // historical done rows from previous sessions flood the tray.
          const liveItems = items.filter(
            (i) => i.status === "queued" || i.status === "running",
          );
          const liveIds = new Set(liveItems.map((i) => `queue-${i.id}`));
          const cancelledIds = new Set(
            items
              .filter((i) => i.status === "cancelled")
              .map((i) => `queue-${i.id}`),
          );
          const doneItems = items.filter((i) => i.status === "done");
          const doneMap = new Map(doneItems.map((i) => [`queue-${i.id}`, i]));
          // Items that failed before emitting any simulation_progress events
          // (e.g. model file unreadable, parse error). In that case no
          // simulation_progress(failed=true) ever fires, so the task would
          // remain stuck as "running". We patch it to "failed" here instead.
          const failedItems = items.filter((i) => i.status === "failed");
          const failedMap = new Map(
            failedItems.map((i) => [`queue-${i.id}`, i]),
          );
          const resolvedProjectName =
            projectsRef.current.find((p) => p.id === projectId)?.name ??
            projectId;

          const fresh: Task[] = liveItems.map((item) => {
            // Preserve live progress fields from an existing task entry so
            // the UI doesn't flash when the queue status update arrives.
            const existing = prev.find((t) => t.id === `queue-${item.id}`);
            const status: Task["status"] =
              item.status === "running" ? "running" : "queued";
            return {
              id: `queue-${item.id}`,
              projectId: projectId,
              scenarioId: item.targetId,
              projectName: resolvedProjectName,
              scenarioName: item.targetName ?? "Base Model",
              status,
              timeLabel: status === "running" ? "Running…" : "Queued",
              progressPercent: existing?.progressPercent,
              progressMessage:
                status === "running"
                  ? (existing?.progressMessage ?? "Solving…")
                  : undefined,
              phase: existing?.phase,
              simulatedSeconds: existing?.simulatedSeconds,
              durationSeconds: existing?.durationSeconds,
              history: existing?.history,
              primaryAction: undefined,
            };
          });

          // Keep everything that isn't being rebuilt and wasn't cancelled.
          // This preserves completed/failed tasks that progress events updated,
          // and any unrelated (non-queue) tasks.
          // Queue truth wins: if the backend marks an item done/failed but the
          // corresponding simulation_progress terminal event was missed, force
          // the UI row to settled state so it cannot remain stuck at 0%.
          const kept = prev
            .filter((t) => !liveIds.has(t.id) && !cancelledIds.has(t.id))
            .map((t) => {
              const doneItem = doneMap.get(t.id);
              if (
                doneItem &&
                (t.status === "running" || t.status === "queued")
              ) {
                return {
                  ...t,
                  status: "completed" as const,
                  timeLabel: `Completed ${finishTimeLabel(doneItem.finishedAt)}`,
                  progressPercent: 100,
                  progressMessage: undefined,
                  primaryAction: "View results" as const,
                };
              }

              if (t.status !== "running" && t.status !== "queued") return t;
              const failedItem = failedMap.get(t.id);
              if (!failedItem) return t;
              return {
                ...t,
                status: "failed" as const,
                timeLabel: `Failed ${finishTimeLabel(failedItem.finishedAt)}`,
                progressMessage: undefined,
                errorMessage: failedItem.error ?? "Simulation failed",
              };
            });

          // Backfill tasks synthesised by the progress-event timing race
          // (progress arrived before run_queue_update created the entry, e.g.
          // when a run finishes so quickly the item is already "done" here).
          // Those placeholders carry neither names nor identity: the progress
          // event names only a run id. `taskBackfill` documents why identity
          // is patched independently of the names — keying on the names alone
          // is what left "View results" dead.
          return [...fresh, ...kept].map((t) => {
            if (!taskNeedsBackfill(t)) return t;
            const matchingItem = items.find((i) => `queue-${i.id}` === t.id);
            if (!matchingItem) return t;
            return backfillTask(t, {
              projectId,
              targetId: matchingItem.targetId,
              projectName: resolvedProjectName,
              targetName: matchingItem.targetName ?? null,
            });
          });
        });
      });
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [clearEdited, bumpScenarios, bumpProjects, refreshResultsFor]);

  const dismissTask = useCallback((id: string) => {
    setTasks((prev) => prev.filter((t) => t.id !== id));
  }, []);

  // Memoized so provider re-renders caused by unrelated app state (toasts,
  // navigation, rail toggles) don't invalidate every useSimulation consumer.
  const simCtxValue = useMemo<SimulationCtxValue>(
    () => ({
      pumpEnergy,
      setPumpEnergy,
      resultMeta,
      resultMetaLoading,
      setResultMeta,
      resultGeneration,
      resultsTopologyStale,
      liveNetworkDigest,
      tasks,
      issues,
      setIssues,
      dismissTask,
    }),
    [
      pumpEnergy,
      resultMeta,
      resultMetaLoading,
      resultGeneration,
      resultsTopologyStale,
      liveNetworkDigest,
      tasks,
      issues,
      dismissTask,
    ],
  );

  return <SimCtx.Provider value={simCtxValue}>{children}</SimCtx.Provider>;
}

export function useSimulation(): SimulationCtxValue {
  return useContext(SimCtx);
}

export function useTasks() {
  return useContext(SimCtx).tasks;
}
