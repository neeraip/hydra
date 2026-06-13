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
import {
  ACCENT,
  loadProjectNetwork,
  type Project,
  type ProjectView,
  useProject,
  useProjects,
} from "./hooks";
import { useNetworkVersion } from "./hooks/NetworkVersionContext";

export type Page = "home" | "projects" | "project" | "settings";
export type { ProjectView } from "./hooks";

/** A point in the in-app navigation history. */
export interface NavLocation {
  page: Page;
  projectView: ProjectView;
  activeProjectId: string | null;
  activeScenarioId: string | null;
}

interface AppState {
  page: Page;
  projectView: ProjectView;
  railOpen: boolean;
  commandPaletteOpen: boolean;
  runModalOpen: boolean;
  scenariosModalOpen: boolean;
  taskTrayOpen: boolean;
  issuesPanelOpen: boolean;
  theme: "dark" | "light" | "system";
  activeProjectId: string | null;
  toast: {
    id: string;
    message: string;
    type: "info" | "success" | "warn" | "error";
  } | null;
  /** Project created in-session via the New Project wizard. Cleared on closeProject. */
  createdProject: Project | null;
  /** True when a real INP file has been loaded via the wizard. */
  isNetworkLoaded: boolean;
  /** Bumped whenever the on-disk project list mutates (create/delete/rename), so
   *  `useProjects` can refetch without a global event bus. */
  projectsVersion: number;
  /** ID of the scenario the user is actively viewing/running. */
  activeScenarioId: string | null;
  /** Bumped whenever the scenario list mutates so `useScenarios` refetches. */
  scenariosVersion: number;
  /** In-app navigation history stack (browser-style back/forward). */
  navHistory: NavLocation[];
  /** Index of the currently visible location in navHistory. */
  navCursor: number;
}

interface AppActions {
  setPage: (page: Page) => void;
  setProjectView: (view: ProjectView) => void;
  openProject: (id: string) => void;
  closeProject: () => void;
  toggleRail: () => void;
  openCommandPalette: () => void;
  closeCommandPalette: () => void;
  openRunModal: () => void;
  closeRunModal: () => void;
  openScenariosModal: () => void;
  closeScenariosModal: () => void;
  toggleTaskTray: () => void;
  openTaskTray: () => void;
  closeTaskTray: () => void;
  toggleIssuesPanel: () => void;
  openIssuesPanel: () => void;
  closeIssuesPanel: () => void;
  setTheme: (theme: "dark" | "light" | "system") => void;
  showToast: (
    message: string,
    type?: "info" | "success" | "warn" | "error",
  ) => void;
  dismissToast: () => void;
  /** Create a project from the wizard (sets createdProject, navigates to canvas). */
  createProject: (p: Project) => void;
  /** Open a previously persisted project (sets createdProject, navigates to overview). */
  enterLoadedProject: (p: Project) => void;
  /** Trigger a re-fetch of the persisted project list. */
  bumpProjects: () => void;
  /** Set which scenario is active (shown in canvas / used for run). */
  setActiveScenarioId: (id: string | null) => void;
  /** Trigger a re-fetch of the scenario list. */
  bumpScenarios: () => void;
  /** Navigate to the previous location (like a browser back button). */
  navBack: () => void;
  /** Navigate to the next location (like a browser forward button). */
  navForward: () => void;
  /** True when there is a previous location to navigate back to. */
  canNavBack: boolean;
  /** True when there is a next location to navigate forward to. */
  canNavForward: boolean;
}

const Ctx = createContext<AppState & AppActions>(null!);

/** Push a new location onto the history stack, discarding any forward entries. */
function pushNav(
  prev: AppState,
  newLoc: NavLocation,
): Pick<AppState, "navHistory" | "navCursor"> {
  const cur = prev.navHistory[prev.navCursor];
  if (
    cur &&
    cur.page === newLoc.page &&
    cur.projectView === newLoc.projectView &&
    cur.activeProjectId === newLoc.activeProjectId &&
    cur.activeScenarioId === newLoc.activeScenarioId
  ) {
    return { navHistory: prev.navHistory, navCursor: prev.navCursor };
  }
  const history = prev.navHistory.slice(0, prev.navCursor + 1);
  return { navHistory: [...history, newLoc], navCursor: history.length };
}

const STORAGE_THEME = "hydra2-theme";
const railOpenKey = (id: string) => `hydra2-rail-open:${id}`;
function readRailOpen(id: string, fallback = true): boolean {
  const v = localStorage.getItem(railOpenKey(id));
  return v === null ? fallback : v === "1";
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [s, setS] = useState<AppState>(() => ({
    page: "home",
    projectView: "canvas",
    railOpen: false,
    commandPaletteOpen: false,
    runModalOpen: false,
    scenariosModalOpen: false,
    taskTrayOpen: false,
    issuesPanelOpen: false,
    theme:
      (localStorage.getItem(STORAGE_THEME) as "dark" | "light" | "system") ??
      "system",
    activeProjectId: null,
    toast: null,
    createdProject: null,
    isNetworkLoaded: false,
    projectsVersion: 0,
    activeScenarioId: null,
    scenariosVersion: 0,
    navHistory: [
      {
        page: "home",
        projectView: "canvas",
        activeProjectId: null,
        activeScenarioId: null,
      },
    ],
    navCursor: 0,
  }));

  useEffect(() => {
    const resolved =
      s.theme === "system"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : s.theme;
    document.documentElement.setAttribute("data-theme", resolved);
    localStorage.setItem(STORAGE_THEME, s.theme);
  }, [s.theme]);

  // When "system" is selected, keep the attribute in sync with OS changes.
  useEffect(() => {
    if (s.theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => {
      document.documentElement.setAttribute(
        "data-theme",
        e.matches ? "dark" : "light",
      );
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [s.theme]);

  // Reload NetworkState whenever the active project or scenario changes so that
  // `useNodes()` / `useLinks()` and the canvas automatically pick up the right INP.
  const { bumpNetwork } = useNetworkVersion();
  useEffect(() => {
    if (!s.activeProjectId) return;
    let cancelled = false;
    const projectId = s.activeProjectId;
    const scenarioId = s.activeScenarioId;

    const delay = (ms: number) =>
      new Promise<void>((resolve) => {
        window.setTimeout(resolve, ms);
      });

    const loadWithRetry = async (
      targetScenarioId: string | null,
      attempts = 3,
    ) => {
      for (let i = 0; i < attempts; i += 1) {
        const net = await loadProjectNetwork(projectId, targetScenarioId);
        if (cancelled) return null;
        if (net !== null) return net;
        if (i < attempts - 1) {
          await delay(120 * (i + 1));
          if (cancelled) return null;
        }
      }
      return null;
    };

    void (async () => {
      const net = await loadWithRetry(scenarioId);
      if (cancelled) return;
      if (net !== null) {
        bumpNetwork();
        return;
      }

      // Recover to base model if a scenario-specific load fails.
      if (scenarioId !== null) {
        const baseNet = await loadWithRetry(null);
        if (cancelled) return;
        if (baseNet !== null) {
          setS((prev) => {
            if (
              prev.activeProjectId !== projectId ||
              prev.activeScenarioId !== scenarioId
            )
              return prev;
            return { ...prev, activeScenarioId: null };
          });
          bumpNetwork();
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [s.activeProjectId, s.activeScenarioId, bumpNetwork]);

  const setPage = useCallback((page: Page) => {
    setS((prev) => {
      const nav = pushNav(prev, {
        page,
        projectView: prev.projectView,
        activeProjectId: prev.activeProjectId,
        activeScenarioId: prev.activeScenarioId,
      });
      return {
        ...prev,
        ...nav,
        page,
        railOpen: page === "project" ? prev.railOpen : false,
        taskTrayOpen: false,
      };
    });
  }, []);

  const setProjectView = useCallback((view: ProjectView) => {
    setS((prev) => {
      if (prev.page === "project" && prev.projectView === view) {
        const next = !prev.railOpen;
        if (prev.activeProjectId)
          localStorage.setItem(
            railOpenKey(prev.activeProjectId),
            next ? "1" : "0",
          );
        return { ...prev, railOpen: next };
      }
      if (prev.activeProjectId) {
        localStorage.setItem(
          `hydra2-project-view:${prev.activeProjectId}`,
          view,
        );
      }
      const nav = pushNav(prev, {
        page: prev.page,
        projectView: view,
        activeProjectId: prev.activeProjectId,
        activeScenarioId: prev.activeScenarioId,
      });
      return { ...prev, ...nav, projectView: view, railOpen: true };
    });
  }, []);

  const openProject = useCallback((id: string) => {
    const stored = localStorage.getItem(
      `hydra2-project-view:${id}`,
    ) as ProjectView | null;
    const projectView: ProjectView = stored ?? "canvas";
    const railOpen = readRailOpen(id);
    setS((prev) => {
      const newLoc = {
        page: "project" as Page,
        projectView,
        activeProjectId: id,
        activeScenarioId: null,
      };
      const nav = pushNav(prev, newLoc);
      return {
        ...prev,
        ...nav,
        page: "project",
        activeProjectId: id,
        activeScenarioId: null,
        projectView,
        railOpen,
        commandPaletteOpen: false,
        taskTrayOpen: false,
      };
    });
  }, []);

  const closeProject = useCallback(() => {
    setS((prev) => {
      const newLoc = {
        page: "projects" as Page,
        projectView: prev.projectView,
        activeProjectId: null,
        activeScenarioId: null,
      };
      const nav = pushNav(prev, newLoc);
      return {
        ...prev,
        ...nav,
        page: "projects",
        activeProjectId: null,
        activeScenarioId: null,
        scenariosVersion: 0,
        railOpen: false,
        createdProject: null,
        isNetworkLoaded: false,
      };
    });
  }, []);

  const createProject = useCallback((p: Project) => {
    setS((prev) => {
      const newLoc = {
        page: "project" as Page,
        projectView: "canvas" as ProjectView,
        activeProjectId: p.id,
        activeScenarioId: null,
      };
      const nav = pushNav(prev, newLoc);
      return {
        ...prev,
        ...nav,
        page: "project",
        activeProjectId: p.id,
        activeScenarioId: null,
        projectView: "canvas",
        railOpen: true,
        commandPaletteOpen: false,
        taskTrayOpen: false,
        createdProject: p,
        isNetworkLoaded: p.nodeCount > 0,
      };
    });
  }, []);

  const enterLoadedProject = useCallback((p: Project) => {
    const stored = localStorage.getItem(
      `hydra2-project-view:${p.id}`,
    ) as ProjectView | null;
    const projectView: ProjectView = stored ?? "overview";
    const railOpen = readRailOpen(p.id);
    setS((prev) => {
      const newLoc = {
        page: "project" as Page,
        projectView,
        activeProjectId: p.id,
        activeScenarioId: null,
      };
      const nav = pushNav(prev, newLoc);
      return {
        ...prev,
        ...nav,
        page: "project",
        activeProjectId: p.id,
        activeScenarioId: null,
        projectView,
        railOpen,
        commandPaletteOpen: false,
        taskTrayOpen: false,
        createdProject: p,
        isNetworkLoaded: p.nodeCount > 0,
      };
    });
  }, []);

  const bumpProjects = useCallback(() => {
    setS((prev) => ({ ...prev, projectsVersion: prev.projectsVersion + 1 }));
  }, []);

  const setActiveScenarioId = useCallback((id: string | null) => {
    setS((prev) => ({ ...prev, activeScenarioId: id }));
  }, []);

  const bumpScenarios = useCallback(() => {
    setS((prev) => ({ ...prev, scenariosVersion: prev.scenariosVersion + 1 }));
  }, []);

  const navBack = useCallback(() => {
    setS((prev) => {
      if (prev.navCursor <= 0) return prev;
      const newCursor = prev.navCursor - 1;
      const loc = prev.navHistory[newCursor];
      return {
        ...prev,
        navCursor: newCursor,
        page: loc.page,
        projectView: loc.projectView,
        activeProjectId: loc.activeProjectId,
        activeScenarioId: loc.activeScenarioId,
        railOpen: loc.page === "project" ? prev.railOpen : false,
      };
    });
  }, []);

  const navForward = useCallback(() => {
    setS((prev) => {
      if (prev.navCursor >= prev.navHistory.length - 1) return prev;
      const newCursor = prev.navCursor + 1;
      const loc = prev.navHistory[newCursor];
      return {
        ...prev,
        navCursor: newCursor,
        page: loc.page,
        projectView: loc.projectView,
        activeProjectId: loc.activeProjectId,
        activeScenarioId: loc.activeScenarioId,
        railOpen: loc.page === "project" ? prev.railOpen : false,
      };
    });
  }, []);

  const toggleRail = useCallback(() => {
    setS((prev) => {
      const next = !prev.railOpen;
      if (prev.activeProjectId)
        localStorage.setItem(
          railOpenKey(prev.activeProjectId),
          next ? "1" : "0",
        );
      return { ...prev, railOpen: next };
    });
  }, []);

  const openCommandPalette = useCallback(() => {
    setS((prev) => ({
      ...prev,
      commandPaletteOpen: true,
      taskTrayOpen: false,
    }));
  }, []);

  const closeCommandPalette = useCallback(() => {
    setS((prev) => ({ ...prev, commandPaletteOpen: false }));
  }, []);

  const openRunModal = useCallback(() => {
    setS((prev) => ({
      ...prev,
      runModalOpen: true,
      commandPaletteOpen: false,
      taskTrayOpen: false,
    }));
  }, []);

  const closeRunModal = useCallback(() => {
    setS((prev) => ({ ...prev, runModalOpen: false }));
  }, []);

  const openScenariosModal = useCallback(() => {
    setS((prev) => ({
      ...prev,
      scenariosModalOpen: true,
      commandPaletteOpen: false,
    }));
  }, []);

  const closeScenariosModal = useCallback(() => {
    setS((prev) => ({ ...prev, scenariosModalOpen: false }));
  }, []);

  const toggleTaskTray = useCallback(() => {
    setS((prev) => ({
      ...prev,
      taskTrayOpen: !prev.taskTrayOpen,
      commandPaletteOpen: false,
    }));
  }, []);

  const openTaskTray = useCallback(() => {
    setS((prev) => ({
      ...prev,
      taskTrayOpen: true,
      commandPaletteOpen: false,
    }));
  }, []);

  const closeTaskTray = useCallback(() => {
    setS((prev) => ({ ...prev, taskTrayOpen: false }));
  }, []);

  const setTheme = useCallback((theme: "dark" | "light" | "system") => {
    setS((prev) => ({ ...prev, theme }));
  }, []);

  const showToast = useCallback(
    (message: string, type: "info" | "success" | "warn" | "error" = "info") => {
      setS((prev) => ({
        ...prev,
        toast: { id: String(Date.now()), message, type },
      }));
    },
    [],
  );

  const dismissToast = useCallback(() => {
    setS((prev) => ({ ...prev, toast: null }));
  }, []);

  const toggleIssuesPanel = useCallback(() => {
    setS((prev) => ({ ...prev, issuesPanelOpen: !prev.issuesPanelOpen }));
  }, []);
  const openIssuesPanel = useCallback(() => {
    setS((prev) => ({ ...prev, issuesPanelOpen: true }));
  }, []);
  const closeIssuesPanel = useCallback(() => {
    setS((prev) => ({ ...prev, issuesPanelOpen: false }));
  }, []);

  return (
    <Ctx.Provider
      value={{
        ...s,
        setPage,
        setProjectView,
        openProject,
        closeProject,
        toggleRail,
        openCommandPalette,
        closeCommandPalette,
        openRunModal,
        closeRunModal,
        openScenariosModal,
        closeScenariosModal,
        toggleTaskTray,
        openTaskTray,
        closeTaskTray,
        toggleIssuesPanel,
        openIssuesPanel,
        closeIssuesPanel,
        setTheme,
        showToast,
        dismissToast,
        createProject,
        enterLoadedProject,
        bumpProjects,
        setActiveScenarioId,
        bumpScenarios,
        navBack,
        navForward,
        canNavBack: s.navCursor > 0,
        canNavForward: s.navCursor < s.navHistory.length - 1,
      }}
    >
      <SimulationProvider>{children}</SimulationProvider>
    </Ctx.Provider>
  );
}

export function useAppState() {
  return useContext(Ctx);
}

/**
 * Derived selector for the active project and its accent color.
 *
 * `accent` always resolves to a string — falls back to the CSS accent token
 * when no project is open, so callers can render confidently.
 */
export interface ActiveProject {
  project: Project | null;
  /** Engine accent color (hex). Falls back to the CSS `--accent` token. */
  accent: string;
}

const FALLBACK_ACCENT = "var(--accent)";

export function useActiveProject(): ActiveProject {
  const { activeProjectId, createdProject, projectsVersion } = useAppState();
  const lookedUpProject = useProject(activeProjectId, projectsVersion);
  const project = lookedUpProject ?? createdProject ?? null;
  return useMemo<ActiveProject>(
    () => ({
      project,
      accent: project ? ACCENT : FALLBACK_ACCENT,
    }),
    [project],
  );
}

import type {
  Issue,
  PumpEnergyRecord,
  ResultMeta,
  RunQueueItem,
  Task,
} from "./hooks";
import {
  runSimulation as _runSimulation,
  getPumpEnergy,
  getRunQueue,
  listenRunQueueUpdate,
  listenSimulationProgress,
  loadResultMeta,
} from "./hooks";

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
  tasks: Task[];
  issues: Issue[];
  setIssues: Dispatch<SetStateAction<Issue[]>>;
  /** Run the simulation, managing a task entry for the duration. */
  runSim: (
    projectName: string,
    scenarioName: string,
    opts?: {
      projectId?: string;
      scenarioId?: string;
      qualityMode?: string;
      traceNode?: string;
    },
  ) => Promise<void>;
  /** Remove a completed or failed task from the tray. */
  dismissTask: (id: string) => void;
}

const SimCtx = createContext<SimulationCtxValue>({
  pumpEnergy: null,
  setPumpEnergy: () => {},
  resultMeta: null,
  resultMetaLoading: false,
  setResultMeta: () => {},
  tasks: [],
  issues: [],
  setIssues: () => {},
  runSim: async () => {},
  dismissTask: () => {},
});

function SimulationProvider({ children }: { children: ReactNode }) {
  const {
    bumpProjects,
    bumpScenarios,
    projectsVersion,
    activeProjectId,
    activeScenarioId,
  } = useAppState();
  const { clearEdited } = useNetworkVersion();
  const [pumpEnergy, setPumpEnergy] = useState<PumpEnergyRecord[] | null>(null);
  const [resultMeta, setResultMeta] = useState<ResultMeta | null>(null);
  const [resultMetaLoading, setResultMetaLoading] = useState(false);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [issues, setIssues] = useState<Issue[]>([]);

  // When the active *project* changes, immediately clear stale metadata so we
  // never show one project's result ranges while a different project is loading.
  // Scenario-only switches do NOT clear here — keeping the stale metadata
  // prevents transient nulls from causing deck.gl layer re-initialisation,
  // inspector card unmounts, and the timeline-height CSS variable flip.
  useEffect(() => {
    setResultMeta(null);
    setPumpEnergy(null);
  }, []);

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
        if (!cancelled) setResultMeta(meta);
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

  // Subscribe to backend simulation_progress events and pipe them into the
  // running task so the TaskTray shows live %, phase label, and progress bar.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listenSimulationProgress((ev) => {
      setTasks((prev) => {
        // Locate the target task:
        //  • Queue path: match by run_id → task id = "queue-{runId}"
        //  • Direct path: first running task (run_id is null)
        let tasks = prev;
        let idx =
          ev.runId != null
            ? prev.findIndex((t) => t.id === `queue-${ev.runId}`)
            : prev.findIndex((t) => t.status === "running");

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
          ev.runId != null &&
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
            hydraulicsDone = true;
            hydraulicsPercent = 100;
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
                timeLabel: `Completed ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`,
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
        const pid = activeProjectIdRef.current;
        const sid = activeScenarioIdRef.current;
        loadResultMeta(pid, sid).then((meta) => {
          if (!cancelled && meta) setResultMeta(meta);
        });
        getPumpEnergy(pid, sid).then((energy) => {
          if (!cancelled && energy) setPumpEnergy(energy);
        });
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
  }, []);

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
        // successfully. This is the queue path (enqueueRuns) — the direct
        // runSim path clears it separately inside its own try/catch.
        for (const item of items) {
          if (item.status === "done") clearEdited(item.targetId);
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
              projectName:
                projectsRef.current.find((p) => p.id === projectId)?.name ??
                projectId,
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
                const finishedAt =
                  doneItem.finishedAt != null
                    ? new Date(doneItem.finishedAt * 1000)
                    : new Date();
                const completedAt = finishedAt.toLocaleTimeString([], {
                  hour: "2-digit",
                  minute: "2-digit",
                });
                return {
                  ...t,
                  status: "completed" as const,
                  timeLabel: `Completed ${completedAt}`,
                  progressPercent: 100,
                  progressMessage: undefined,
                  primaryAction: "View results" as const,
                };
              }

              if (t.status !== "running" && t.status !== "queued") return t;
              const failedItem = failedMap.get(t.id);
              if (!failedItem) return t;
              const finishedAt =
                failedItem.finishedAt != null
                  ? new Date(failedItem.finishedAt * 1000)
                  : new Date();
              const failedAt = finishedAt.toLocaleTimeString([], {
                hour: "2-digit",
                minute: "2-digit",
              });
              return {
                ...t,
                status: "failed" as const,
                timeLabel: `Failed ${failedAt}`,
                progressMessage: undefined,
                errorMessage: failedItem.error ?? "Simulation failed",
              };
            });

          // Patch placeholder names on tasks that were synthesised before
          // run_queue_update arrived (e.g. when the simulation completes so
          // quickly that the item is already "done" when this handler runs,
          // so it was never included in liveItems above).
          const resolvedProjectName =
            projectsRef.current.find((p) => p.id === projectId)?.name ??
            projectId;
          return [...fresh, ...kept].map((t) => {
            if (t.projectName !== "…" && t.scenarioName !== "…") return t;
            const matchingItem = items.find((i) => `queue-${i.id}` === t.id);
            if (!matchingItem) return t;
            return {
              ...t,
              projectName:
                t.projectName === "…" ? resolvedProjectName : t.projectName,
              scenarioName:
                t.scenarioName === "…"
                  ? (matchingItem.targetName ?? "Base Model")
                  : t.scenarioName,
            };
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
  }, [
    clearEdited,
    bumpScenarios, // run_queue_update always fires after the backend has committed the
    // new run state (running → done/failed) to the DB, so this is the
    // correct place to refresh project and scenario state badges.
    bumpProjects,
  ]);

  const runSim = useCallback(
    async (
      projectName: string,
      scenarioName: string,
      opts?: {
        projectId?: string;
        scenarioId?: string;
        qualityMode?: string;
        traceNode?: string;
      },
    ): Promise<void> => {
      const id = `task-${Date.now()}`;
      const startedAt = new Date().toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });
      setTasks((prev) => [
        {
          id,
          projectName,
          scenarioName,
          status: "running",
          timeLabel: `Started ${startedAt}`,
          progressPercent: undefined,
          progressMessage: "Solving…",
          history: [{ at: Date.now(), label: "Queued" }],
        },
        ...prev,
      ]);
      try {
        const result = await _runSimulation(opts);
        const elapsed = new Date().toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
        });
        if (result) {
          // Store only pump energy (epilog data — tiny regardless of network size).
          // Cross-period analytics are fetched on-demand by individual views.
          setPumpEnergy(result.pumpEnergy);
          // Clear the stale flag for this scenario now that results are fresh.
          clearEdited(opts?.scenarioId ?? null);
          // Load global metadata (snapshot times + ranges) from the binary
          // results file so the Timeline and ResultsSummary work without loading
          // all periods into memory.
          if (opts?.projectId) {
            loadResultMeta(opts.projectId, opts.scenarioId).then((meta) => {
              if (meta) setResultMeta(meta);
            });
          }
          // Refresh project and scenario rows so state badges update immediately.
          bumpProjects();
          bumpScenarios();
          setTasks((prev) =>
            prev.map((t) =>
              t.id === id
                ? {
                    ...t,
                    status: "completed",
                    timeLabel: `Completed ${elapsed}`,
                    primaryAction: "View results",
                  }
                : t,
            ),
          );
        } else {
          setTasks((prev) =>
            prev.map((t) =>
              t.id === id
                ? {
                    ...t,
                    status: "failed",
                    timeLabel: `Failed ${elapsed}`,
                    errorMessage:
                      "Simulation returned no results. Is a network loaded?",
                  }
                : t,
            ),
          );
        }
      } catch (err) {
        const elapsed = new Date().toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
        });
        setTasks((prev) =>
          prev.map((t) =>
            t.id === id
              ? {
                  ...t,
                  status: "failed",
                  timeLabel: `Failed ${elapsed}`,
                  errorMessage: String(err),
                }
              : t,
          ),
        );
      }
    },
    [bumpProjects, bumpScenarios, clearEdited],
  );

  const dismissTask = useCallback((id: string) => {
    setTasks((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <SimCtx.Provider
      value={{
        pumpEnergy,
        setPumpEnergy,
        resultMeta,
        resultMetaLoading,
        setResultMeta,
        tasks,
        issues,
        setIssues,
        runSim,
        dismissTask,
      }}
    >
      {children}
    </SimCtx.Provider>
  );
}

export function useSimulation(): SimulationCtxValue {
  return useContext(SimCtx);
}

export function useTasks() {
  return useContext(SimCtx).tasks;
}
