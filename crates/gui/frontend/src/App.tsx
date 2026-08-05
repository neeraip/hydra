import { lazy, Suspense, useEffect, useRef, useState } from "react";
import {
  getDraftDirtyCount,
  type Page,
  saveDraftsViaGuard,
  useAppState,
} from "./AppContext";
import { registerCustomCrsDefinitions } from "./canvas/coords";
import { useCanvasSelection } from "./canvas/selection-context";
import { ActivityBar } from "./components/layout/ActivityBar";
import { StatusBar } from "./components/layout/StatusBar";
import { TopBar } from "./components/layout/TopBar";
import { ClearResultsModal } from "./components/modals/ClearResultsModal";
import { IssuesPanel } from "./components/panels/IssuesPanel";
import { TaskTray } from "./components/panels/TaskTray";
import { Toast } from "./components/ui/Toast";
import { TooltipPortal } from "./components/ui/TooltipPortal";
import { tryInvoke } from "./hooks/ipc";
import { useUndoRedo } from "./hooks/useUndoRedo";
import { loadSettingsDrawer, whenIdle } from "./lazyChunks";
import { startMainThreadStallWatch } from "./perfTrace";
import type { ProjectView } from "./projectConfig";
import {
  isEditableEventTarget,
  PROJECTS_SEARCH_INPUT_ID,
  primaryModifierPressed,
} from "./shortcuts";

const CommandPalette = lazy(() =>
  import("./components/modals/CommandPalette").then((m) => ({
    default: m.CommandPalette,
  })),
);
const RunModal = lazy(() =>
  import("./components/modals/RunModal").then((m) => ({ default: m.RunModal })),
);
const SimSettingsModal = lazy(() =>
  import("./components/modals/SimSettingsModal").then((m) => ({
    default: m.SimSettingsModal,
  })),
);
const ScenariosModal = lazy(() =>
  import("./components/modals/ScenariosModal").then((m) => ({
    default: m.ScenariosModal,
  })),
);
const CrsModal = lazy(() =>
  import("./components/modals/CrsPicker").then((m) => ({
    default: m.CrsPicker,
  })),
);
const BasemapProvidersModal = lazy(() =>
  import("./components/modals/BasemapProvidersModal").then((m) => ({
    default: m.BasemapProvidersModal,
  })),
);
const ShortcutCard = lazy(() =>
  import("./components/modals/ShortcutCard").then((m) => ({
    default: m.ShortcutCard,
  })),
);
const HomePage = lazy(() =>
  import("./pages/HomePage").then((m) => ({ default: m.HomePage })),
);
const ProjectPage = lazy(() =>
  import("./pages/project/ProjectPage").then((m) => ({
    default: m.ProjectPage,
  })),
);
const SettingsDrawer = lazy(() =>
  loadSettingsDrawer().then((m) => ({ default: m.SettingsDrawer })),
);
const ProjectsPage = lazy(() =>
  import("./pages/ProjectsPage").then((m) => ({ default: m.ProjectsPage })),
);

// Drives the page-transition direction. Settings is absent because it is
// an overlay now — it slides in over whatever page is showing rather than
// replacing it, so it has no place in a left-to-right ordering.
const PAGE_ORDER: Page[] = ["home", "projects", "project"];

// ⌘1–⌘4 project view shortcuts (also advertised in CommandPalette hints).
const VIEW_SHORTCUTS: Record<string, ProjectView> = {
  "1": "overview",
  "2": "canvas",
  "3": "editor",
  "4": "analysis",
};

export function App() {
  const {
    page,
    commandPaletteOpen,
    openCommandPalette,
    closeCommandPalette,
    runModalOpen,
    openRunModal,
    closeRunModal,
    simSettingsModalOpen,
    activeProjectId,
    taskTrayOpen,
    settingsOpen,
    setProjectView,
    issuesPanelOpen,
    toggleIssuesPanel,
    closeIssuesPanel,
  } = useAppState();

  // Selection is provider state mounted above AppProvider, so it survives
  // every project switch on its own — nothing but deleting an element ever
  // cleared it. Leaving it set meant the inspector stayed open on an element
  // from the project you just left, showing another project's data under it
  // whenever the id happened to exist in both.
  //
  // Cleared here because App is the only always-mounted component inside both
  // the selection provider and AppProvider; the provider itself sits outside
  // AppProvider and cannot read the active project.
  const { clearSelection } = useCanvasSelection();
  // biome-ignore lint/correctness/useExhaustiveDependencies: the project id is the intentional reset trigger; clearSelection is stable.
  useEffect(() => {
    clearSelection();
  }, [activeProjectId]);

  const { undo, redo } = useUndoRedo();
  const [shortcutCardOpen, setShortcutCardOpen] = useState(false);
  const [animKey, setAnimKey] = useState(0);
  const [animDir, setAnimDir] = useState<"right" | "left">("right");
  const prevPageRef = useRef<Page>(page);
  const isFirstRender = useRef(true);

  // Dev-only: log any main-thread stall >250ms so perf regressions on huge
  // networks show up as `[hydra-perf] main-thread-stall` console lines.
  useEffect(() => startMainThreadStallWatch(), []);

  // Warm the Settings chunk once the app has settled. It is small and most
  // sessions open it at least once, so paying for it during an idle moment
  // costs nothing and removes the pause that otherwise lands on the click.
  useEffect(() => whenIdle(() => void loadSettingsDrawer()), []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      // Overlay user-defined custom CRS on top — these take precedence because
      // proj4.defs() overwrites any existing entry for the same code.
      const defs =
        await tryInvoke<Array<{ label: string; epsg: string; proj4: string }>>(
          "list_custom_crs",
        );
      if (cancelled || !defs) return;
      registerCustomCrsDefinitions(defs);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      prevPageRef.current = page;
      return;
    }
    const prevIdx = PAGE_ORDER.indexOf(prevPageRef.current);
    const currIdx = PAGE_ORDER.indexOf(page);
    setAnimDir(currIdx >= prevIdx ? "right" : "left");
    setAnimKey((k) => k + 1);
    prevPageRef.current = page;
  }, [page]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const key = e.key.toLowerCase();
      const primary = primaryModifierPressed(e);
      // Non-modifier single-key shortcuts must not fire while typing in an
      // input/textarea/select/contentEditable.
      const inEditable = isEditableEventTarget(e.target);

      if (primary && key === "k") {
        e.preventDefault();
        commandPaletteOpen ? closeCommandPalette() : openCommandPalette();
      }
      const projectOpen = page === "project" && activeProjectId != null;
      // Focuses the canvas view, then broadcasts a canvas command event.
      const canvasCommand = (event: string, detail: string) => {
        e.preventDefault();
        setProjectView("canvas");
        window.dispatchEvent(new CustomEvent(event, { detail }));
      };

      // ⌘R / Ctrl-R — open Run modal (only when a project is open)
      if (primary && key === "r" && projectOpen) {
        e.preventDefault();
        runModalOpen ? closeRunModal() : openRunModal();
      }
      // ⌘M / Ctrl-M — toggle canvas geographic/orthogonal layout
      if (primary && key === "m" && !e.shiftKey && projectOpen) {
        canvasCommand("hydra:canvas-layout", "toggle");
      }
      // ⌘= zoom in, ⌘- zoom out, ⌘0 fit to network extent
      const viewportAction =
        key === "=" || key === "+"
          ? "zoom-in"
          : key === "-" || key === "_"
            ? "zoom-out"
            : key === "0"
              ? "fit"
              : null;
      if (primary && viewportAction && projectOpen) {
        canvasCommand("hydra:canvas-viewport", viewportAction);
      }
      if (
        (key === "?" || (e.shiftKey && key === "/")) &&
        !primary &&
        !e.altKey &&
        !inEditable
      ) {
        setShortcutCardOpen((prev) => !prev);
      }
      // ⌘Z undo / ⌘⇧Z redo — committed network edits, project page only.
      // Skipped while typing (text fields have their own undo) and while the
      // command palette or run modal is open (their own undo semantics).
      if (
        primary &&
        key === "z" &&
        page === "project" &&
        !inEditable &&
        !commandPaletteOpen &&
        !runModalOpen
      ) {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
      }
      // ⌘S / Ctrl-S — save staged editor drafts. preventDefault always on
      // the project page so the browser save dialog can never appear.
      if (primary && key === "s" && page === "project") {
        e.preventDefault();
        if (getDraftDirtyCount() > 0) void saveDraftsViaGuard();
      }
      // ⌘F / Ctrl-F — focus the Projects page search input.
      if (primary && key === "f" && page === "projects") {
        e.preventDefault();
        document.getElementById(PROJECTS_SEARCH_INPUT_ID)?.focus();
      }
      // ⌘⇧M / Ctrl-Shift-M — toggle issues panel
      if (primary && e.shiftKey && key === "m") {
        if (activeProjectId) {
          e.preventDefault();
          toggleIssuesPanel();
        }
      }
      if (e.key === "Escape") {
        if (shortcutCardOpen) {
          setShortcutCardOpen(false);
        } else if (issuesPanelOpen) {
          closeIssuesPanel();
        } else if (commandPaletteOpen) {
          closeCommandPalette();
        }
      }
      // Project-scoped view shortcuts:
      // ⌘1 Overview, ⌘2 Canvas, ⌘3 Editor, ⌘4 Analysis.
      if (primary && page === "project") {
        const view = VIEW_SHORTCUTS[key];
        if (view) {
          e.preventDefault();
          setProjectView(view);
        }
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    commandPaletteOpen,
    openCommandPalette,
    closeCommandPalette,
    runModalOpen,
    openRunModal,
    closeRunModal,
    activeProjectId,
    page,
    setProjectView,
    shortcutCardOpen,
    issuesPanelOpen,
    toggleIssuesPanel,
    closeIssuesPanel,
    undo,
    redo,
  ]);

  // Alternate between identical base/-alt keyframe names so the animation
  // restarts on each navigation WITHOUT remounting the page subtree — the
  // previous key-based remount mounted the whole ProjectPage twice per open.
  const slideAnim =
    animKey > 0
      ? `${animDir === "right" ? "slideInRight" : "slideInLeft"}${animKey % 2 === 0 ? "Alt" : ""} 280ms cubic-bezier(0.25, 0.46, 0.45, 0.94)`
      : undefined;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "row",
        width: "100%",
        height: "100%",
        overflow: "hidden",
        position: "relative",
      }}
    >
      <ActivityBar />
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        <TopBar />
        <div
          style={{
            flex: 1,
            display: "flex",
            overflow: "hidden",
            position: "relative",
          }}
        >
          <div
            style={{
              flex: 1,
              overflow: "hidden",
              display: "flex",
              flexDirection: "column",
            }}
          >
            <div
              style={{
                flex: 1,
                overflow: "hidden",
                display: "flex",
                flexDirection: "column",
                animation: slideAnim,
              }}
            >
              <Suspense fallback={<PageLoader />}>
                {page === "home" && <HomePage />}
                {page === "projects" && <ProjectsPage />}
                {page === "project" && <ProjectPage />}
              </Suspense>
            </div>
          </div>
        </div>
        <StatusBar />
      </div>
      {commandPaletteOpen && (
        <Suspense fallback={null}>
          <CommandPalette />
        </Suspense>
      )}
      <ClearResultsModal />
      {runModalOpen && (
        <Suspense fallback={null}>
          <RunModal />
        </Suspense>
      )}
      {simSettingsModalOpen && (
        <Suspense fallback={null}>
          <SimSettingsModal />
        </Suspense>
      )}
      <Suspense fallback={null}>
        <ScenariosModal />
      </Suspense>
      <Suspense fallback={null}>
        <CrsModal />
      </Suspense>
      {/* Below the modals it can itself open — BasemapProviders sits at
          205, above this drawer's 200. */}
      {settingsOpen && (
        <Suspense fallback={null}>
          <SettingsDrawer />
        </Suspense>
      )}
      <Suspense fallback={null}>
        <BasemapProvidersModal />
      </Suspense>
      {shortcutCardOpen && (
        <Suspense fallback={null}>
          <ShortcutCard onClose={() => setShortcutCardOpen(false)} />
        </Suspense>
      )}
      {taskTrayOpen && <TaskTray />}
      <IssuesPanel />
      <Toast />
      <TooltipPortal />
    </div>
  );
}

function PageLoader() {
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: "var(--text-tertiary)",
        fontSize: "var(--text-lg)",
      }}
    >
      Loading…
    </div>
  );
}
