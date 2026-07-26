import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ACCENT,
  fetchProjectsShared,
  loadProjectNetwork,
  type Project,
  type ProjectView,
  useProject,
} from "./hooks";
import { formatIpcError, isTauri, onIpcError } from "./hooks/ipc";
import { useNetworkData } from "./hooks/NetworkDataContext";
import { useNetworkVersion } from "./hooks/NetworkVersionContext";
import { startPerfSpan } from "./perfTrace";
import { SimulationProvider } from "./SimulationContext";

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
  simSettingsModalOpen: boolean;
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
  /** Pending request to reveal a specific element in the Network Editor
   *  (set by "Open in editor" from the canvas inspector). `nonce` bumps on
   *  every request so the editor re-runs the jump even for the same id. */
  editorFocus: { kind: string; id: string; nonce: number } | null;
}

interface AppActions {
  /** Formatted error from the last failed model load (e.g. the INP was
   * hand-edited outside Hydra and no longer parses); null once a load
   * succeeds. Feeds a persistent Issues-panel entry. */
  networkLoadFailure: string | null;
  setPage: (page: Page) => void;
  setProjectView: (view: ProjectView) => void;
  /** Navigate to the Network Editor and reveal `id` (scroll + select its row).
   *  `kind` is the element kind ("junction" | "pipe" | …). */
  focusInEditor: (kind: string, id: string) => void;
  /** Open/switch to a project. Pass `view` to land on a specific tab (the
   *  ProjectSwitcher passes the current view to preserve it across a switch);
   *  omit it to resume the project's last-active tab. */
  openProject: (id: string, view?: ProjectView) => void;
  closeProject: () => void;
  toggleRail: () => void;
  openCommandPalette: () => void;
  closeCommandPalette: () => void;
  openRunModal: () => void;
  closeRunModal: () => void;
  openSimSettingsModal: () => void;
  closeSimSettingsModal: () => void;
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

// ── Session restore ─────────────────────────────────────────────────────────
const STORAGE_LAST_PROJECT = "hydra2-last-project";
const STORAGE_RESTORE_SESSION = "hydra2-restore-session";
/** "Reopen last project on launch" — enabled unless explicitly turned off. */
function readRestoreSession(): boolean {
  return localStorage.getItem(STORAGE_RESTORE_SESSION) !== "false";
}
/** The project to reopen on launch, or null when disabled / none stored. */
function restoreProjectId(): string | null {
  return readRestoreSession()
    ? localStorage.getItem(STORAGE_LAST_PROJECT)
    : null;
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [s, setS] = useState<AppState>(() => {
    const base: AppState = {
      page: "home",
      projectView: "canvas",
      railOpen: false,
      commandPaletteOpen: false,
      runModalOpen: false,
      simSettingsModalOpen: false,
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
      editorFocus: null,
    };
    // Session restore: launch straight into the last-open project. The
    // network-load effect keys on activeProjectId, so seeding it here loads
    // the model automatically; a deleted project falls back to Home via the
    // validation effect below.
    const restoreId = restoreProjectId();
    if (!restoreId) return base;
    const projectView = readProjectView(restoreId) ?? "canvas";
    return {
      ...base,
      page: "project",
      projectView,
      activeProjectId: restoreId,
      railOpen: readRailOpen(restoreId),
      navHistory: [
        {
          page: "project",
          projectView,
          activeProjectId: restoreId,
          activeScenarioId: null,
        },
      ],
    };
  });

  // Live snapshot of state for imperative reads inside stable callbacks
  // (navigation guards need the *current* page without re-creating the
  // callbacks on every state change).
  const sRef = useRef(s);
  useEffect(() => {
    sRef.current = s;
  });

  // Session restore: remember the open project so the next launch can reopen
  // it, and clear it whenever the user leaves for a non-project page.
  useEffect(() => {
    if (s.page === "project" && s.activeProjectId) {
      localStorage.setItem(STORAGE_LAST_PROJECT, s.activeProjectId);
    } else if (s.page !== "project") {
      localStorage.removeItem(STORAGE_LAST_PROJECT);
    }
  }, [s.page, s.activeProjectId]);

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
  const { primeNetworkData, clearNetworkData } = useNetworkData();
  // Which project the network store currently holds (or is loading) data for.
  // Lets the load effect clear stale arrays exactly once per project switch —
  // and NOT on scenario switches, where keeping the old geometry visible while
  // the sibling scenario loads is deliberate.
  const networkDataProjectRef = useRef<string | null>(null);
  useEffect(() => {
    if (!s.activeProjectId) return;
    let cancelled = false;
    const projectId = s.activeProjectId;
    const scenarioId = s.activeScenarioId;

    // Project switch: drop the previous project's arrays immediately so no
    // consumer (Overview composition, canvas, editor) renders the old
    // network's data while this project's snapshot is still loading.
    if (networkDataProjectRef.current !== projectId) {
      networkDataProjectRef.current = projectId;
      clearNetworkData();
    }

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
          setNetworkLoadFailure(null);
          bumpNetwork();
          return;
        }

        // Recover to base model if a scenario-specific load fails.
        if (scenarioId !== null) {
          const baseNet = await loadWithRetry(null);
          if (cancelled) return;
          if (baseNet !== null) {
            primeNetworkData(baseNet);
            setNetworkLoadFailure(null);
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
    clearNetworkData,
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

  const focusInEditor = useCallback((kind: string, id: string) => {
    setS((prev) => {
      const nonce = (prev.editorFocus?.nonce ?? 0) + 1;
      const focus = { kind, id, nonce };
      // Already on the editor: just bump the focus request (do NOT toggle the
      // rail the way setProjectView("editor") would).
      if (prev.page === "project" && prev.projectView === "editor") {
        return { ...prev, editorFocus: focus };
      }
      if (prev.activeProjectId) {
        localStorage.setItem(projectViewKey(prev.activeProjectId), "editor");
      }
      const nav = pushNav(prev, {
        page: prev.page,
        projectView: "editor",
        activeProjectId: prev.activeProjectId,
        activeScenarioId: prev.activeScenarioId,
      });
      const railOpen = prev.activeProjectId
        ? readRailOpen(prev.activeProjectId)
        : prev.railOpen;
      return {
        ...prev,
        ...nav,
        projectView: "editor",
        railOpen,
        editorFocus: focus,
      };
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
    (id: string, view?: ProjectView) => {
      goToProject(
        id,
        view ?? readProjectView(id) ?? "canvas",
        readRailOpen(id),
      );
    },
    [goToProject],
  );

  // Just navigates to "projects" — setPage centrally clears all
  // project-scoped state (activeProjectId, isNetworkLoaded, etc.) whenever
  // the destination isn't "project", so there's nothing extra to do here.
  const closeProject = useCallback(() => {
    setPage("projects");
  }, [setPage]);

  // Session restore fallback: when we launched straight into a restored
  // project, verify it still exists once the project list resolves and drop
  // back to Home if it was deleted since the last session. One-shot.
  useEffect(() => {
    const id = restoreProjectId();
    if (!id) return;
    let cancelled = false;
    fetchProjectsShared().then((rows) => {
      if (cancelled || rows === null) return;
      if (!rows.some((p) => p.id === id)) {
        localStorage.removeItem(STORAGE_LAST_PROJECT);
        setPage("projects");
      }
    });
    return () => {
      cancelled = true;
    };
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

  const openSimSettingsModal = useCallback(() => {
    setS((prev) => ({
      ...prev,
      simSettingsModalOpen: true,
      runModalOpen: false,
      commandPaletteOpen: false,
    }));
  }, []);

  const closeSimSettingsModal = useCallback(() => {
    setS((prev) => ({ ...prev, simSettingsModalOpen: false }));
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
  // Persistent record of a failed model load (e.g. the INP was hand-edited
  // outside Hydra and no longer parses). Toasts vanish; this feeds a durable
  // issue in the Issues panel until the next successful load clears it.
  const [networkLoadFailure, setNetworkLoadFailure] = useState<string | null>(
    null,
  );
  useEffect(
    () =>
      onIpcError((cmd, err) => {
        const message = `Backend error (${cmd}): ${formatIpcError(err)}`;
        if (cmd === "load_project_network" || cmd === "get_network_snapshot") {
          setNetworkLoadFailure(formatIpcError(err));
        }
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

  // Tab flips within a project are deferred so the nav highlight paints
  // immediately while the heavy subtree switch happens in an interruptible
  // render. Deferring the view ALONE leaked across project opens, though: this
  // provider is always mounted, so for one deferred beat it still served the
  // previous view ("canvas" on a cold start), flashing the wrong page before
  // the project's saved view appeared. Defer the (project, view) pair and
  // discard the deferred snapshot when it belongs to a different project —
  // project opens render their saved view immediately; tab flips stay deferred.
  const viewSnapshot = useMemo(
    () => ({ projectId: s.activeProjectId, view: s.projectView }),
    [s.activeProjectId, s.projectView],
  );
  const deferredViewSnapshot = useDeferredValue(viewSnapshot);
  const deferredProjectView =
    deferredViewSnapshot.projectId === s.activeProjectId
      ? deferredViewSnapshot.view
      : s.projectView;

  // Memoized: this provider re-renders on every piece of app state (toasts,
  // nav, modals); an inline value object handed every useAppState consumer a
  // fresh reference each time, re-rendering the whole tree per state change.
  // All callbacks are stable useCallbacks, so `s` is the only real dependency.
  const appValue = useMemo(
    () => ({
      ...s,
      networkLoadFailure,
      setPage,
      setProjectView,
      focusInEditor,
      openProject,
      closeProject,
      toggleRail,
      openCommandPalette,
      closeCommandPalette,
      openRunModal,
      closeRunModal,
      openSimSettingsModal,
      closeSimSettingsModal,
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
      networkLoadFailure,
      setPage,
      setProjectView,
      focusInEditor,
      openProject,
      closeProject,
      toggleRail,
      openCommandPalette,
      closeCommandPalette,
      openRunModal,
      closeRunModal,
      openSimSettingsModal,
      closeSimSettingsModal,
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

// Re-exported for consumers that import from AppContext.
export { useSimulation, useTasks } from "./SimulationContext";
