import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  createContext,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
  useCallback,
  useContext,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useCanvasStatus } from "./canvas/status-context";
import {
  ACCENT,
  fetchValidationFindings,
  loadProjectNetwork,
  type Project,
  type ProjectView,
  useProject,
  useProjects,
  validationFindingsToIssues,
} from "./hooks";
import { formatIpcError, isTauri, onIpcError } from "./hooks/ipc";
import { useNetworkData } from "./hooks/NetworkDataContext";
import { useNetworkVersion } from "./hooks/NetworkVersionContext";
import { startPerfSpan } from "./perfTrace";

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
  crsModalOpen: boolean;
  taskTrayOpen: boolean;
  issuesPanelOpen: boolean;
  theme: "dark" | "light" | "system";
  activeProjectId: string | null;
  /** Visible toast stack, newest first (capped at MAX_TOASTS). */
  toasts: {
    id: string;
    message: string;
    type: "info" | "success" | "warn" | "error";
  }[];
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
  /** Bumped whenever `update_sim_params` succeeds so `useSimParams` refetches. */
  simParamsVersion: number;
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
  openCrsModal: () => void;
  closeCrsModal: () => void;
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
  dismissToast: (id: string) => void;
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
  /** Trigger a re-fetch of simulation parameters (after update_sim_params). */
  bumpSimParams: () => void;
  /** Navigate to the previous location (like a browser back button). */
  navBack: () => void;
  /** Navigate to the next location (like a browser forward button). */
  navForward: () => void;
  /** True when there is a previous location to navigate back to. */
  canNavBack: boolean;
  /** True when there is a next location to navigate forward to. */
  canNavForward: boolean;
  /** `projectView`, one transition behind: consumers that gate expensive
   * subtrees (view mounts, editor row models, canvas activation) read this so
   * a tab click paints the highlight immediately while the heavy subtree flip
   * happens in an interruptible deferred render. */
  deferredProjectView: ProjectView;
}

const Ctx = createContext<(AppState & AppActions) | null>(null);

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

/** Window within which identical backend-error toasts are suppressed. */
const IPC_TOAST_DEDUPE_MS = 5000;

/** Maximum number of simultaneously visible toasts (newest wins). */
const MAX_TOASTS = 4;

// ── Draft guard seam ────────────────────────────────────────────────────────
//
// DraftContext lives *below* AppProvider (it is mounted by NetworkEditor and
// itself consumes useAppState), so AppContext cannot read it through a hook —
// and importing DraftContext here would create a module cycle. Instead
// DraftContext registers a tiny imperative API at mount time; navigation
// handlers and the window-close guard read it on demand.

export interface DraftGuard {
  /** Total staged (unsaved) editor changes right now. */
  getDirtyCount: () => number;
  /** Save every staged change — same path as the editor save bar. */
  saveAll: () => Promise<{ applied: number; failed: number; errors: string[] }>;
}

let draftGuard: DraftGuard | null = null;

/** Called by DraftProvider on mount; returns an unregister function. */
export function registerDraftGuard(guard: DraftGuard): () => void {
  draftGuard = guard;
  return () => {
    if (draftGuard === guard) draftGuard = null;
  };
}

/** Current staged editor change count (0 when no editor draft exists). */
export function getDraftDirtyCount(): number {
  return draftGuard?.getDirtyCount() ?? 0;
}

/** Save staged editor drafts via the registered guard (no-op without one). */
export function saveDraftsViaGuard(): Promise<{
  applied: number;
  failed: number;
  errors: string[];
}> | null {
  return draftGuard ? draftGuard.saveAll() : null;
}

/**
 * Ask the user to confirm leaving/closing with unsaved editor drafts.
 * Returns `true` when navigation may proceed. Some webviews don't implement
 * `window.confirm` (it returns `undefined`); only an explicit `false` blocks
 * the action so navigation/close can never be wedged.
 */
function confirmDiscardDrafts(verb: string): boolean {
  const n = getDraftDirtyCount();
  if (n === 0) return true;
  const res = window.confirm(
    `You have ${n} unsaved editor change${n === 1 ? "" : "s"}. ${verb} anyway and discard ${n === 1 ? "it" : "them"}?`,
  );
  return res !== false;
}

const STORAGE_THEME = "hydra2-theme";
const railOpenKey = (id: string) => `hydra2-rail-open:${id}`;
function readRailOpen(id: string): boolean {
  const v = localStorage.getItem(railOpenKey(id));
  return v === null ? true : v === "1";
}

const projectViewKey = (id: string) => `hydra2-project-view:${id}`;
/** Last-used view for a project, persisted by `setProjectView`. */
function readProjectView(id: string): ProjectView | null {
  return localStorage.getItem(projectViewKey(id)) as ProjectView | null;
}

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

export function AppProvider({ children }: { children: ReactNode }) {
  const [s, setS] = useState<AppState>(() => ({
    page: "home",
    projectView: "canvas",
    railOpen: false,
    commandPaletteOpen: false,
    runModalOpen: false,
    scenariosModalOpen: false,
    crsModalOpen: false,
    taskTrayOpen: false,
    issuesPanelOpen: false,
    theme:
      (localStorage.getItem(STORAGE_THEME) as "dark" | "light" | "system") ??
      "system",
    activeProjectId: null,
    toasts: [],
    createdProject: null,
    isNetworkLoaded: false,
    projectsVersion: 0,
    activeScenarioId: null,
    scenariosVersion: 0,
    simParamsVersion: 0,
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

  // Live snapshot of state for imperative reads inside stable callbacks
  // (navigation guards need the *current* page without re-creating the
  // callbacks on every state change).
  const sRef = useRef(s);
  useEffect(() => {
    sRef.current = s;
  });

  // Tauri window-close guard: prompt when editor drafts are dirty. Outside a
  // Tauri shell (plain vite dev server) this effect is a no-op.
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    getCurrentWindow()
      .onCloseRequested((event) => {
        if (!confirmDiscardDrafts("Close")) event.preventDefault();
      })
      .then((fn) => {
        // StrictMode double-mount: dispose a late-resolving listener instead
        // of leaking it.
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((err) => {
        console.warn("[app] failed to register close guard:", err);
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

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

  const showToast = useCallback(
    (message: string, type: "info" | "success" | "warn" | "error" = "info") => {
      // Toasts auto-dismiss, so in dev keep a durable copy of error/warn
      // messages in the console for inspection after the toast is gone.
      if (import.meta.env.DEV && (type === "error" || type === "warn")) {
        const log = type === "error" ? console.error : console.warn;
        log(`[toast:${type}] ${message}`);
      }
      // Unique id generated outside the updater (StrictMode double-invokes
      // updaters, so they must stay pure).
      const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      setS((prev) => ({
        ...prev,
        toasts: [{ id, message, type }, ...prev.toasts].slice(0, MAX_TOASTS),
      }));
    },
    [],
  );

  // Reload NetworkState whenever the active project or scenario changes so that
  // `useNodes()` / `useLinks()` and the canvas automatically pick up the right INP.
  const { bumpNetwork } = useNetworkVersion();
  const { primeNetworkData } = useNetworkData();
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
      const loadSpan = startPerfSpan("network-load-with-retry", {
        projectId,
        scenarioId: targetScenarioId ?? "base",
        maxAttempts: attempts,
      });
      try {
        for (let i = 0; i < attempts; i += 1) {
          const attemptSpan = startPerfSpan("network-load-attempt", {
            projectId,
            scenarioId: targetScenarioId ?? "base",
            attempt: i + 1,
          });
          let snapshot: Awaited<ReturnType<typeof loadProjectNetwork>>;
          try {
            snapshot = await loadProjectNetwork(projectId, targetScenarioId);
          } catch (err) {
            // Decode failure (frontend/backend layout mismatch) — not
            // retryable; end the span and let the outer catch surface it.
            attemptSpan.end({ loaded: false, error: true });
            throw err;
          }
          attemptSpan.end({ loaded: snapshot !== null });
          if (cancelled) return null;
          if (snapshot !== null) {
            loadSpan.end({ loaded: true, attempt: i + 1 });
            return snapshot;
          }
          if (i < attempts - 1) {
            await delay(120 * (i + 1));
            if (cancelled) return null;
          }
        }
        loadSpan.end({ loaded: false });
        return null;
      } catch (err) {
        loadSpan.end({ loaded: false, error: true });
        throw err;
      }
    };

    void (async () => {
      try {
        const net = await loadWithRetry(scenarioId);
        if (cancelled) return;
        if (net !== null) {
          primeNetworkData(net);
          bumpNetwork();
          return;
        }

        // Recover to base model if a scenario-specific load fails.
        if (scenarioId !== null) {
          const baseNet = await loadWithRetry(null);
          if (cancelled) return;
          if (baseNet !== null) {
            primeNetworkData(baseNet);
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
      } catch (err) {
        // `loadProjectNetwork` throws on snapshot decode failures — without
        // this catch the async IIFE turned them into unhandled rejections.
        if (cancelled) return;
        console.error("[network] load_project_network failed:", err);
        showToast(`Failed to load network: ${formatIpcError(err)}`, "error");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [
    s.activeProjectId,
    s.activeScenarioId,
    bumpNetwork,
    primeNetworkData,
    showToast,
  ]);

  const setPage = useCallback((page: Page) => {
    // Guard: leaving the project page discards any staged editor drafts
    // (DraftProvider unmounts with the editor). Confirm before proceeding.
    if (
      sRef.current.page === "project" &&
      page !== "project" &&
      !confirmDiscardDrafts("Leave")
    ) {
      return;
    }
    setS((prev) => {
      // Leaving the project view must always clear project-scoped state,
      // regardless of which call site triggered the navigation. Enforced
      // here centrally (rather than requiring every caller to remember to
      // use closeProject) so future nav entry points can't regress this.
      const activeProjectId = page === "project" ? prev.activeProjectId : null;
      const activeScenarioId =
        page === "project" ? prev.activeScenarioId : null;
      const leavingProject =
        page !== "project" && prev.activeProjectId !== null;

      const nav = pushNav(prev, {
        page,
        projectView: prev.projectView,
        activeProjectId,
        activeScenarioId,
      });
      return {
        ...prev,
        ...nav,
        page,
        activeProjectId,
        activeScenarioId,
        railOpen: page === "project" ? prev.railOpen : false,
        taskTrayOpen: false,
        ...(leavingProject
          ? {
              scenariosVersion: 0,
              createdProject: null,
              isNetworkLoaded: false,
            }
          : {}),
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
        localStorage.setItem(projectViewKey(prev.activeProjectId), view);
      }
      const nav = pushNav(prev, {
        page: prev.page,
        projectView: view,
        activeProjectId: prev.activeProjectId,
        activeScenarioId: prev.activeScenarioId,
      });
      // Restore the persisted per-project rail preference rather than forcing
      // the rail open: force-opening flipped needSimObjects and rebuilt the
      // 92k merged sim-object arrays on every view switch.
      const railOpen = prev.activeProjectId
        ? readRailOpen(prev.activeProjectId)
        : prev.railOpen;
      return { ...prev, ...nav, projectView: view, railOpen };
    });
  }, []);

  /** Shared transition for every "enter a project" entry point: navigate to
   *  the project page, reset scenario/palette/tray state, and apply the
   *  per-entry-point extras (wizard-created project fields). */
  const goToProject = useCallback(
    (
      id: string,
      projectView: ProjectView,
      railOpen: boolean,
      extra?: Pick<AppState, "createdProject" | "isNetworkLoaded">,
    ) => {
      setS((prev) => {
        const nav = pushNav(prev, {
          page: "project",
          projectView,
          activeProjectId: id,
          activeScenarioId: null,
        });
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
          ...extra,
        };
      });
    },
    [],
  );

  const openProject = useCallback(
    (id: string) => {
      goToProject(id, readProjectView(id) ?? "canvas", readRailOpen(id));
    },
    [goToProject],
  );

  // Just navigates to "projects" — setPage centrally clears all
  // project-scoped state (activeProjectId, isNetworkLoaded, etc.) whenever
  // the destination isn't "project", so there's nothing extra to do here.
  const closeProject = useCallback(() => {
    setPage("projects");
  }, [setPage]);

  const createProject = useCallback(
    (p: Project) => {
      goToProject(p.id, "canvas", true, {
        createdProject: p,
        isNetworkLoaded: p.nodeCount > 0,
      });
    },
    [goToProject],
  );

  const enterLoadedProject = useCallback(
    (p: Project) => {
      goToProject(
        p.id,
        readProjectView(p.id) ?? "overview",
        readRailOpen(p.id),
        { createdProject: p, isNetworkLoaded: p.nodeCount > 0 },
      );
    },
    [goToProject],
  );

  const bumpProjects = useCallback(() => {
    setS((prev) => ({ ...prev, projectsVersion: prev.projectsVersion + 1 }));
  }, []);

  const setActiveScenarioId = useCallback((id: string | null) => {
    setS((prev) => ({ ...prev, activeScenarioId: id }));
  }, []);

  const bumpScenarios = useCallback(() => {
    setS((prev) => ({ ...prev, scenariosVersion: prev.scenariosVersion + 1 }));
  }, []);

  const bumpSimParams = useCallback(() => {
    setS((prev) => ({ ...prev, simParamsVersion: prev.simParamsVersion + 1 }));
  }, []);

  /** Move the nav cursor by ±1 and restore that history location (no-op at
   *  either end of the stack). */
  const navBy = useCallback((delta: -1 | 1) => {
    // Same unsaved-drafts guard as setPage, applied to back/forward
    // navigation that would leave the project page.
    {
      const cur = sRef.current;
      const targetCursor = cur.navCursor + delta;
      if (targetCursor < 0 || targetCursor >= cur.navHistory.length) return;
      const target = cur.navHistory[targetCursor];
      if (
        cur.page === "project" &&
        target.page !== "project" &&
        !confirmDiscardDrafts("Leave")
      ) {
        return;
      }
    }
    setS((prev) => {
      const newCursor = prev.navCursor + delta;
      if (newCursor < 0 || newCursor >= prev.navHistory.length) return prev;
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

  const navBack = useCallback(() => navBy(-1), [navBy]);
  const navForward = useCallback(() => navBy(1), [navBy]);

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

  const openCrsModal = useCallback(() => {
    setS((prev) => ({
      ...prev,
      crsModalOpen: true,
      commandPaletteOpen: false,
    }));
  }, []);

  const closeCrsModal = useCallback(() => {
    setS((prev) => ({ ...prev, crsModalOpen: false }));
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

  const dismissToast = useCallback((id: string) => {
    setS((prev) => ({
      ...prev,
      toasts: prev.toasts.filter((t) => t.id !== id),
    }));
  }, []);

  // Surface real backend IPC failures from the otherwise-silent `tryInvoke`
  // reads (e.g. a corrupted app-data DB making `list_projects` fail) so they
  // don't masquerade as empty data. Only fires inside a Tauri shell.
  //
  // Deduped: the network-load retry loop can hit the same failing command up
  // to six times in a row (3 scenario attempts + 3 base-fallback attempts),
  // which previously stacked six identical error toasts. One toast per
  // identical message within the window is enough; a persistent failure
  // resurfaces once the window elapses.
  const recentIpcToastRef = useRef<{ message: string; at: number } | null>(
    null,
  );
  useEffect(
    () =>
      onIpcError((cmd, err) => {
        const message = `Backend error (${cmd}): ${formatIpcError(err)}`;
        const now = Date.now();
        const recent = recentIpcToastRef.current;
        if (
          recent &&
          recent.message === message &&
          now - recent.at < IPC_TOAST_DEDUPE_MS
        ) {
          return;
        }
        recentIpcToastRef.current = { message, at: now };
        showToast(message, "error");
      }),
    [showToast],
  );

  const toggleIssuesPanel = useCallback(() => {
    setS((prev) => {
      if (!prev.activeProjectId) return { ...prev, issuesPanelOpen: false };
      return { ...prev, issuesPanelOpen: !prev.issuesPanelOpen };
    });
  }, []);
  const openIssuesPanel = useCallback(() => {
    setS((prev) => {
      if (!prev.activeProjectId) return { ...prev, issuesPanelOpen: false };
      return { ...prev, issuesPanelOpen: true };
    });
  }, []);
  const closeIssuesPanel = useCallback(() => {
    setS((prev) => ({ ...prev, issuesPanelOpen: false }));
  }, []);

  // Never allow Issues drawer without an active project.
  useEffect(() => {
    if (s.activeProjectId) return;
    setS((prev) =>
      prev.issuesPanelOpen ? { ...prev, issuesPanelOpen: false } : prev,
    );
  }, [s.activeProjectId]);

  const deferredProjectView = useDeferredValue(s.projectView);

  // Memoized: this provider re-renders on every piece of app state (toasts,
  // nav, modals); an inline value object handed every useAppState consumer a
  // fresh reference each time, re-rendering the whole tree per state change.
  // All callbacks are stable useCallbacks, so `s` is the only real dependency.
  const appValue = useMemo(
    () => ({
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
      openCrsModal,
      closeCrsModal,
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
      bumpSimParams,
      navBack,
      navForward,
      canNavBack: s.navCursor > 0,
      canNavForward: s.navCursor < s.navHistory.length - 1,
      deferredProjectView,
    }),
    [
      s,
      deferredProjectView,
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
      openCrsModal,
      closeCrsModal,
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
      bumpSimParams,
      navBack,
      navForward,
    ],
  );

  return (
    <Ctx.Provider value={appValue}>
      <SimulationProvider>{children}</SimulationProvider>
    </Ctx.Provider>
  );
}

export function useAppState() {
  const ctx = useContext(Ctx);
  if (!ctx) {
    throw new Error("useAppState must be used within AppProvider");
  }
  return ctx;
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
  /**
   * Opaque freshness token for `resultMeta`: incremented every time result
   * metadata is (re)loaded from disk via `loadResultMeta` — on project/
   * scenario switch and again when a run completes. Consumers caching
   * derived or per-period data keyed on result identity include this in
   * their keys so a re-run that produces value-equal metadata still
   * invalidates the cache.
   */
  resultGeneration: number;
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
  resultGeneration: 0,
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
  const [tasks, setTasks] = useState<Task[]>([]);
  const [issues, setIssues] = useState<Issue[]>([]);
  // Backend `validate_network` findings, already mapped to Issue shape.
  const [validationIssues, setValidationIssues] = useState<Issue[]>([]);

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
    const pushIssue = (
      issue: Omit<Issue, "link" | "firstSeen" | "dismissed">,
    ) => {
      next.push({
        ...issue,
        link: { view: "canvas", label: "Open canvas" },
        firstSeen: firstSeenNow,
        dismissed: false,
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
    // code+elementId keys so the dismissed-merge below persists dismissals).
    next.push(...validationIssues);

    setIssues((prev) => {
      const prevById = new Map(prev.map((i) => [i.id, i]));
      return next.map((i) => {
        const existing = prevById.get(i.id);
        if (!existing) return i;
        return {
          ...i,
          firstSeen: existing.firstSeen,
          dismissed: existing.dismissed,
        };
      });
    });
  }, [
    activeProjectId,
    activeScenarioId,
    coordMissingCount,
    coordStatus,
    coordTotalCount,
    editedScenarioIds,
    resultMeta,
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
        const pid = activeProjectIdRef.current;
        const sid = activeScenarioIdRef.current;
        loadResultMeta(pid, sid).then((meta) => {
          if (!cancelled && meta) {
            setResultMeta(meta);
            setResultGeneration((g) => g + 1);
          }
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

          // Patch placeholder names on tasks that were synthesised before
          // run_queue_update arrived (e.g. when the simulation completes so
          // quickly that the item is already "done" when this handler runs,
          // so it was never included in liveItems above).
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
  }, [clearEdited, bumpScenarios, bumpProjects]);

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
      const startedAt = formatClockTime();
      /** Merge `patch` into this run's task entry. */
      const patchTask = (patch: Partial<Task>) => {
        setTasks((prev) =>
          prev.map((t) => (t.id === id ? { ...t, ...patch } : t)),
        );
      };
      setTasks((prev) => [
        {
          id,
          projectId: opts?.projectId,
          scenarioId: opts?.scenarioId ?? null,
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
        const elapsed = formatClockTime();
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
              if (meta) {
                setResultMeta(meta);
                setResultGeneration((g) => g + 1);
              }
            });
          }
          // Refresh project and scenario rows so state badges update immediately.
          bumpProjects();
          bumpScenarios();
          patchTask({
            status: "completed",
            timeLabel: `Completed ${elapsed}`,
            primaryAction: "View results",
          });
        } else {
          patchTask({
            status: "failed",
            timeLabel: `Failed ${elapsed}`,
            errorMessage:
              "Simulation returned no results. Is a network loaded?",
          });
        }
      } catch (err) {
        patchTask({
          status: "failed",
          timeLabel: `Failed ${formatClockTime()}`,
          errorMessage: String(err),
        });
      }
    },
    [bumpProjects, bumpScenarios, clearEdited],
  );

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
      tasks,
      issues,
      setIssues,
      runSim,
      dismissTask,
    }),
    [
      pumpEnergy,
      resultMeta,
      resultMetaLoading,
      resultGeneration,
      tasks,
      issues,
      runSim,
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
