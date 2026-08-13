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
  type EngineInfo,
  engineByKey,
  fetchProjectsShared,
  loadProjectNetwork,
  type Project,
  type ProjectView,
  useEngines,
  useProject,
} from "./hooks";
import { formatIpcError, onIpcError } from "./hooks/ipc";
import { useNetworkData } from "./hooks/NetworkDataContext";
import { useNetworkVersion } from "./hooks/NetworkVersionContext";
import { startPerfSpan } from "./perfTrace";
import { reselectsCurrentView } from "./projectViewNav";
import { SimulationProvider } from "./SimulationContext";

/**
 * The three places the app can *be*.
 *
 * Settings is deliberately absent: it is an overlay you return from, not a
 * location you travel to. As a page it took part in navigation history —
 * so Back walked you through settings visits — and, worse, visiting it
 * counted as leaving your project, which erased the session restore's
 * memory of what you had open. Both were consequences of calling a detour
 * a destination; neither is expressible now.
 */
export type Page = "home" | "projects" | "project";
export type { ProjectView } from "./hooks";

/** A point in the in-app navigation history. */
export interface NavLocation {
  page: Page;
  projectView: ProjectView;
  activeProjectId: string | null;
  activeScenarioId: string | null;
}

/** What a confirmed clear-results action should delete. */
export interface ClearResultsRequest {
  projectId: string;
  /** Target scope: one scenario (`scenarioId`, null = base model), or the
   * whole project. */
  scope: "target" | "all";
  /** `null` addresses the base model. Ignored when `scope` is "all". */
  scenarioId: string | null;
  /** Human-facing name of what is being cleared, for the confirmation. */
  name: string;
  /** How many targets currently hold results (used by the "all" wording). */
  simulatedCount?: number;
}

interface AppState {
  page: Page;
  projectView: ProjectView;
  railOpen: boolean;
  commandPaletteOpen: boolean;
  /** The query the palette opens with. A shortcut can land the user in a
   *  mode — element search, say — rather than at a blank prompt. */
  commandPaletteQuery: string;
  runModalOpen: boolean;
  simSettingsModalOpen: boolean;
  scenariosModalOpen: boolean;
  /**
   * Pending "clear simulation results" request awaiting confirmation, or
   * null. Lives at app level rather than in the surface that asked, because
   * the command palette unmounts the moment it runs a command and so cannot
   * own a modal of its own — and because one owner means one implementation
   * of the delete, its toasts and its refresh.
   */
  clearResults: ClearResultsRequest | null;
  crsModalOpen: boolean;
  basemapProvidersModalOpen: boolean;
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
  /** Whether the Settings drawer is open. An overlay over whatever page is
   *  underneath, which is left untouched. */
  settingsOpen: boolean;
  /** Whether the keyboard-shortcut card is open. Here rather than local to
   *  `App` so the command palette can offer it — a card whose whole job is
   *  discoverability was itself reachable only by already knowing `?`. */
  shortcutCardOpen: boolean;
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
  /** Be on this view. Unlike `setProjectView`, arriving where you already
   *  are is a no-op rather than the rail gesture. */
  goToProjectView: (view: ProjectView) => void;
  /** Navigate to the Network Editor and reveal `id` (scroll + select its row).
   *  `kind` is the element kind ("junction" | "pipe" | …). */
  focusInEditor: (kind: string, id: string) => void;
  /** Open/switch to a project. Pass `view` to land on a specific tab (the
   *  ProjectSwitcher passes the current view to preserve it across a switch);
   *  omit it to resume the project's last-active tab. */
  openProject: (id: string, view?: ProjectView) => void;
  closeProject: () => void;
  toggleSettings: () => void;
  closeSettings: () => void;
  toggleShortcutCard: () => void;
  closeShortcutCard: () => void;
  toggleRail: () => void;
  /** Opens the palette, optionally with the query already filled in — a
   *  shortcut that lands in a mode rather than at a blank prompt. */
  openCommandPalette: (initialQuery?: string) => void;
  closeCommandPalette: () => void;
  openRunModal: () => void;
  closeRunModal: () => void;
  openSimSettingsModal: () => void;
  closeSimSettingsModal: () => void;
  openScenariosModal: () => void;
  closeScenariosModal: () => void;
  requestClearResults: (req: ClearResultsRequest) => void;
  closeClearResults: () => void;
  openCrsModal: () => void;
  closeCrsModal: () => void;
  openBasemapProvidersModal: () => void;
  closeBasemapProvidersModal: () => void;
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

/**
 * Whether the secondary rail should be open at a navigated-to location.
 *
 * Back/forward must land on the same rail state as arriving any other way,
 * so this reads the target project's saved preference rather than carrying
 * the current one across. Carrying it is what collapsed the network list:
 * leaving the project page sets `railOpen` false (no rail exists there),
 * and navigating back into the project then inherited that false and
 * ignored the preference the user had actually set.
 *
 * The preference itself is global, so crossing from one project to another
 * lands on the same rail state. What is *not* global is whether a rail
 * exists at all: off the project page there is none, which is a different
 * question and the reason this function exists.
 */
export function railOpenForLocation(
  loc: NavLocation,
  savedPref: () => boolean,
): boolean {
  if (loc.page !== "project") return false;
  return loc.activeProjectId ? savedPref() : true;
}

/** Window within which identical backend-error toasts are suppressed. */
const IPC_TOAST_DEDUPE_MS = 5000;

/** Maximum number of simultaneously visible toasts (newest wins). */
const MAX_TOASTS = 4;

const STORAGE_THEME = "hydra2-theme";
/**
 * One key, not one per project.
 *
 * Whether the network list is open is a fact about how someone is working,
 * not about which project they opened — the same way an editor's sidebar
 * does not reopen itself because you changed folder. Kept per project, it
 * meant that stepping through projects from the breadcrumb made the panel
 * open and close on its own, which reads as the app doing something rather
 * than as a preference being honoured.
 *
 * Old `hydra2-rail-open:<id>` entries are left where they are. They are
 * read by nothing now, and picking one project's answer to speak for all
 * of them would be guessing.
 */
const RAIL_OPEN_KEY = "hydra2-rail-open";
function readRailOpen(): boolean {
  const v = localStorage.getItem(RAIL_OPEN_KEY);
  return v === null ? true : v === "1";
}

/** Written whichever project is open, and no longer gated on one being
 *  open at all — the preference outlives the project it was set in. */
function writeRailOpen(open: boolean): void {
  try {
    localStorage.setItem(RAIL_OPEN_KEY, open ? "1" : "0");
  } catch {
    // A panel preference is not worth failing a navigation over.
  }
}

const projectViewKey = (id: string) => `hydra2-project-view:${id}`;

/**
 * Move to a project view: remember it, push history, and restore the rail.
 *
 * Shared by the two entry points, which differ only in what they do when
 * the requested view is already the current one — `setProjectView` reads
 * that as the rail gesture, `goToProjectView` as nothing to do.
 *
 * The rail is restored from its persisted preference rather than forced
 * open: force-opening flipped `needSimObjects` and rebuilt the 92k merged
 * sim-object arrays on every view switch.
 */
function navigateToView(prev: AppState, view: ProjectView) {
  if (prev.activeProjectId) {
    localStorage.setItem(projectViewKey(prev.activeProjectId), view);
  }
  const nav = pushNav(prev, {
    page: prev.page,
    projectView: view,
    activeProjectId: prev.activeProjectId,
    activeScenarioId: prev.activeScenarioId,
  });
  return { ...prev, ...nav, projectView: view, railOpen: readRailOpen() };
}
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
      settingsOpen: false,
      shortcutCardOpen: false,
      railOpen: false,
      commandPaletteOpen: false,
      commandPaletteQuery: "",
      runModalOpen: false,
      simSettingsModalOpen: false,
      scenariosModalOpen: false,
      clearResults: null,
      crsModalOpen: false,
      basemapProvidersModalOpen: false,
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
      railOpen: readRailOpen(),
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

  // Session restore: remember the open project so the next launch can
  // reopen it, and forget it when the user leaves for a page that is not a
  // project — Home or Projects, the only two there are.
  //
  // That "only two" is the point. Settings used to be a page as well, so
  // opening it to change one setting counted as leaving, and the next
  // launch had forgotten the project you were mid-way through. Settings is
  // an overlay now and `Page` has three members, so a detour cannot be
  // mistaken for a departure — not because this condition enumerates the
  // exceptions correctly, but because there are none to enumerate.
  useEffect(() => {
    if (s.page === "project" && s.activeProjectId) {
      localStorage.setItem(STORAGE_LAST_PROJECT, s.activeProjectId);
    } else if (s.page !== "project") {
      localStorage.removeItem(STORAGE_LAST_PROJECT);
    }
  }, [s.page, s.activeProjectId]);

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

        // A base-model load that comes back absent is not a failure: the
        // project has no model yet (freshly created and never imported).
        // Prime an empty snapshot so the store leaves the loading state that
        // `clearNetworkData` entered — without this the app sits in
        // `loading` forever and consumers keep rendering whatever was there.
        if (scenarioId === null) {
          primeNetworkData({ nodes: [], links: [] });
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
      // Reselecting the current tab collapses the rail. See
      // `reselectsCurrentView`, and use `goToProjectView` if you only mean
      // to be on this view.
      if (reselectsCurrentView(prev.page, prev.projectView, view)) {
        const next = !prev.railOpen;
        writeRailOpen(next);
        return { ...prev, railOpen: next };
      }
      return navigateToView(prev, view);
    });
  }, []);

  /**
   * Be on this view, without the reselect meaning.
   *
   * For callers that navigate in order to do something else — a shortcut, a
   * command — where arriving somewhere you already were has to be a no-op
   * rather than a collapsed rail.
   */
  const goToProjectView = useCallback((view: ProjectView) => {
    setS((prev) =>
      reselectsCurrentView(prev.page, prev.projectView, view)
        ? prev
        : navigateToView(prev, view),
    );
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
      const railOpen = readRailOpen();
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
      goToProject(id, view ?? readProjectView(id) ?? "canvas", readRailOpen());
    },
    [goToProject],
  );

  // Just navigates to "projects" — setPage centrally clears all
  // project-scoped state (activeProjectId, isNetworkLoaded, etc.) whenever
  // the destination isn't "project", so there's nothing extra to do here.
  const closeProject = useCallback(() => {
    setPage("projects");
  }, [setPage]);

  const toggleSettings = useCallback(() => {
    setS((prev) => ({ ...prev, settingsOpen: !prev.settingsOpen }));
  }, []);
  const closeSettings = useCallback(() => {
    setS((prev) =>
      prev.settingsOpen ? { ...prev, settingsOpen: false } : prev,
    );
  }, []);
  const toggleShortcutCard = useCallback(() => {
    setS((prev) => ({ ...prev, shortcutCardOpen: !prev.shortcutCardOpen }));
  }, []);
  const closeShortcutCard = useCallback(() => {
    setS((prev) =>
      prev.shortcutCardOpen ? { ...prev, shortcutCardOpen: false } : prev,
    );
  }, []);

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
      goToProject(p.id, readProjectView(p.id) ?? "overview", readRailOpen(), {
        createdProject: p,
        isNetworkLoaded: p.nodeCount > 0,
      });
    },
    [goToProject],
  );

  const bumpProjects = useCallback(() => {
    setS((prev) => ({ ...prev, projectsVersion: prev.projectsVersion + 1 }));
  }, []);

  const setActiveScenarioId = useCallback((id: string | null) => {
    // Same guard as leaving the project page. Staged editor drafts are held
    // per-project, not per-scenario, so a silent switch left them pointing at
    // a model they were not authored against: the next ⌘S applied one
    // target's edits to another's model.inp. Both halves matter — the backend
    // refuses a save whose target does not own the loaded network, and this
    // stops the user reaching that state by accident in the first place.
    if (sRef.current.activeScenarioId === id) return;
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
        railOpen: railOpenForLocation(loc, readRailOpen),
      };
    });
  }, []);

  const navBack = useCallback(() => navBy(-1), [navBy]);
  const navForward = useCallback(() => navBy(1), [navBy]);

  const toggleRail = useCallback(() => {
    setS((prev) => {
      const next = !prev.railOpen;
      writeRailOpen(next);
      return { ...prev, railOpen: next };
    });
  }, []);

  const openCommandPalette = useCallback((initialQuery = "") => {
    setS((prev) => ({
      ...prev,
      commandPaletteOpen: true,
      commandPaletteQuery: initialQuery,
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

  const requestClearResults = useCallback((req: ClearResultsRequest) => {
    // Closing the palette here keeps the confirmation the only thing on
    // screen — the palette is itself an overlay and would sit on top of it.
    setS((prev) => ({
      ...prev,
      clearResults: req,
      commandPaletteOpen: false,
    }));
  }, []);

  const closeClearResults = useCallback(() => {
    setS((prev) => ({ ...prev, clearResults: null }));
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

  const openBasemapProvidersModal = useCallback(() => {
    setS((prev) => ({
      ...prev,
      basemapProvidersModalOpen: true,
      commandPaletteOpen: false,
    }));
  }, []);

  const closeBasemapProvidersModal = useCallback(() => {
    setS((prev) => ({ ...prev, basemapProvidersModalOpen: false }));
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
      goToProjectView,
      focusInEditor,
      openProject,
      closeProject,
      toggleSettings,
      closeSettings,
      toggleShortcutCard,
      closeShortcutCard,
      toggleRail,
      openCommandPalette,
      closeCommandPalette,
      openRunModal,
      closeRunModal,
      openSimSettingsModal,
      closeSimSettingsModal,
      openScenariosModal,
      closeScenariosModal,
      requestClearResults,
      closeClearResults,
      openCrsModal,
      closeCrsModal,
      openBasemapProvidersModal,
      closeBasemapProvidersModal,
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
      goToProjectView,
      focusInEditor,
      openProject,
      closeProject,
      toggleSettings,
      closeSettings,
      toggleShortcutCard,
      closeShortcutCard,
      toggleRail,
      openCommandPalette,
      closeCommandPalette,
      openRunModal,
      closeRunModal,
      openSimSettingsModal,
      closeSimSettingsModal,
      openScenariosModal,
      closeScenariosModal,
      requestClearResults,
      closeClearResults,
      openCrsModal,
      closeCrsModal,
      openBasemapProvidersModal,
      closeBasemapProvidersModal,
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
 * Derived selector for the active project, its engine, and accent color.
 *
 * `engine` is the registry descriptor for the project's engine key — null
 * when no project is open, and also null for an unresolvable key (a
 * project from a newer Hydra): render that as unsupported, never as a
 * default engine. `accent` always resolves to a string — falls back to the
 * CSS accent token — so callers can render confidently.
 */
export interface ActiveProject {
  project: Project | null;
  /** Registry descriptor of the project's engine; null when no project is
   * open or the key is unsupported by this build. */
  engine: EngineInfo | null;
  /** Engine accent color (hex). Falls back to the CSS `--accent` token. */
  accent: string;
}

const FALLBACK_ACCENT = "var(--accent)";

export function useActiveProject(): ActiveProject {
  const { activeProjectId, createdProject, projectsVersion } = useAppState();
  const lookedUpProject = useProject(activeProjectId, projectsVersion);
  const engines = useEngines();
  const project = lookedUpProject ?? createdProject ?? null;
  return useMemo<ActiveProject>(() => {
    const engine = project ? engineByKey(engines, project.engine) : null;
    return {
      project,
      engine,
      accent: engine?.accent ?? FALLBACK_ACCENT,
    };
  }, [project, engines]);
}

// Re-exported for consumers that import from AppContext.
export { useSimulation, useTasks } from "./SimulationContext";
