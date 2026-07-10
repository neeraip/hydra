import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { type Page, useAppState } from "./AppContext";
import { registerCustomCrsDefinitions } from "./canvas/coords";
import { ActivityBar } from "./components/layout/ActivityBar";
import { StatusBar } from "./components/layout/StatusBar";
import { TopBar } from "./components/layout/TopBar";
import { IssuesPanel } from "./components/panels/IssuesPanel";
import { TaskTray } from "./components/panels/TaskTray";
import { Toast } from "./components/ui/Toast";
import { TooltipPortal } from "./components/ui/TooltipPortal";
import { tryInvoke } from "./hooks/ipc";

const CommandPalette = lazy(() =>
  import("./components/modals/CommandPalette").then((m) => ({
    default: m.CommandPalette,
  })),
);
const RunModal = lazy(() =>
  import("./components/modals/RunModal").then((m) => ({ default: m.RunModal })),
);
const ScenariosModal = lazy(() =>
  import("./components/modals/ScenariosModal").then((m) => ({
    default: m.ScenariosModal,
  })),
);
const CrsModal = lazy(() =>
  import("./components/modals/CrsModal").then((m) => ({
    default: m.CrsModal,
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
const SettingsPage = lazy(() =>
  import("./pages/SettingsPage").then((m) => ({ default: m.SettingsPage })),
);
const ProjectsPage = lazy(() =>
  import("./pages/ProjectsPage").then((m) => ({ default: m.ProjectsPage })),
);

const PAGE_ORDER: Page[] = ["home", "projects", "project", "settings"];

export function App() {
  const {
    page,
    commandPaletteOpen,
    openCommandPalette,
    closeCommandPalette,
    runModalOpen,
    openRunModal,
    closeRunModal,
    activeProjectId,
    taskTrayOpen,
    setProjectView,
    issuesPanelOpen,
    toggleIssuesPanel,
    closeIssuesPanel,
  } = useAppState();

  const [shortcutCardOpen, setShortcutCardOpen] = useState(false);
  const [animKey, setAnimKey] = useState(0);
  const [animDir, setAnimDir] = useState<"right" | "left">("right");
  const prevPageRef = useRef<Page>(page);
  const isFirstRender = useRef(true);

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
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        commandPaletteOpen ? closeCommandPalette() : openCommandPalette();
      }
      // ⌘R / Ctrl-R — open Run modal (only when a project is open)
      if ((e.metaKey || e.ctrlKey) && (e.key === "r" || e.key === "R")) {
        if (page === "project" && activeProjectId) {
          e.preventDefault();
          runModalOpen ? closeRunModal() : openRunModal();
        }
      }
      // ⌘M / Ctrl-M — toggle canvas geographic/orthogonal layout
      if ((e.metaKey || e.ctrlKey) && (e.key === "m" || e.key === "M")) {
        if (page === "project" && activeProjectId && !e.shiftKey) {
          e.preventDefault();
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-layout", { detail: "toggle" }),
          );
        }
      }
      // ⌘= / Ctrl-= — zoom in on canvas
      if ((e.metaKey || e.ctrlKey) && e.key === "=") {
        if (page === "project" && activeProjectId) {
          e.preventDefault();
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-viewport", { detail: "zoom-in" }),
          );
        }
      }
      // ⌘- / Ctrl-- — zoom out on canvas
      if ((e.metaKey || e.ctrlKey) && e.key === "-") {
        if (page === "project" && activeProjectId) {
          e.preventDefault();
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-viewport", {
              detail: "zoom-out",
            }),
          );
        }
      }
      // ⌘0 / Ctrl-0 — fit canvas to full network extent
      if ((e.metaKey || e.ctrlKey) && e.key === "0") {
        if (page === "project" && activeProjectId) {
          e.preventDefault();
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-viewport", { detail: "fit" }),
          );
        }
      }
      if (e.key === "?" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        setShortcutCardOpen((prev) => !prev);
      }
      // ⌘⇧M / Ctrl-Shift-M — toggle issues panel
      if (
        (e.metaKey || e.ctrlKey) &&
        e.shiftKey &&
        (e.key === "m" || e.key === "M")
      ) {
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
      // ⌘1 Overview, ⌘2 Canvas, ⌘3 Analysis, ⌘4 Editor.
      if ((e.metaKey || e.ctrlKey) && page === "project") {
        if (e.key === "1") {
          e.preventDefault();
          setProjectView("overview");
        } else if (e.key === "2") {
          e.preventDefault();
          setProjectView("canvas");
        } else if (e.key === "3") {
          e.preventDefault();
          setProjectView("editor");
        } else if (e.key === "4") {
          e.preventDefault();
          setProjectView("analysis");
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
  ]);

  const slideAnim =
    animKey > 0
      ? `${animDir === "right" ? "slideInRight" : "slideInLeft"} 280ms cubic-bezier(0.25, 0.46, 0.45, 0.94)`
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
              key={animKey}
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
                {page === "settings" && <SettingsPage />}
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
      {runModalOpen && (
        <Suspense fallback={null}>
          <RunModal />
        </Suspense>
      )}
      <Suspense fallback={null}>
        <ScenariosModal />
      </Suspense>
      <Suspense fallback={null}>
        <CrsModal />
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
        fontSize: 13,
      }}
    >
      Loading…
    </div>
  );
}
