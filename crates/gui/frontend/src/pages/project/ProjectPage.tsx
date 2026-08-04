import { lazy, Suspense, useEffect, useRef } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { ProjectPeriodProvider } from "../../canvas/period-context";
import { ProjectToolbar } from "../../components/layout/ProjectToolbar";
import { SecondaryRail } from "../../components/layout/SecondaryRail";
import { engineComponents } from "../../engine/registry";
import { startPerfSpan } from "../../perfTrace";

const OverviewView = lazy(() =>
  import("./OverviewView").then((m) => ({ default: m.OverviewView })),
);
const CanvasView = lazy(() =>
  import("./CanvasView").then((m) => ({ default: m.CanvasView })),
);
const NetworkEditor = lazy(() =>
  import("./NetworkEditor").then((m) => ({ default: m.NetworkEditor })),
);
const AnalysisView = lazy(() =>
  import("./AnalysisView").then((m) => ({ default: m.AnalysisView })),
);
const ReportView = lazy(() =>
  import("./ReportView").then((m) => ({ default: m.ReportView })),
);

export function ProjectPage() {
  // Deferred: the tab highlight (TopBar) reads the urgent value; the heavy
  // view subtrees flip one interruptible render later so the click paints
  // instantly even on 46k-element networks.
  const { deferredProjectView: projectView } = useAppState();
  const { project, engine } = useActiveProject();
  const engineViews = engineComponents(engine?.key);
  const EditorView = engineViews.EditorView;
  const EngineAnalysisView = engineViews.AnalysisView;

  // Dev-only: time from a view-tab switch committing to the next painted
  // frame. Shows up as `[hydra-perf] view-switch-paint` with the view name.
  const prevViewRef = useRef(projectView);
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    if (prevViewRef.current === projectView) return;
    prevViewRef.current = projectView;
    const span = startPerfSpan("view-switch-paint", { view: projectView });
    let inner: number | null = null;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => span.end());
    });
    return () => {
      cancelAnimationFrame(outer);
      if (inner != null) cancelAnimationFrame(inner);
    };
  }, [projectView]);

  return (
    <ProjectPeriodProvider>
      <div
        style={{
          flex: 1,
          height: "100%",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          animation: "fadeIn 150ms ease-out",
        }}
      >
        <ProjectToolbar />
        <div
          style={{
            flex: 1,
            position: "relative",
            overflow: "hidden",
            display: "flex",
            flexDirection: "column",
          }}
        >
          <SecondaryRail />
          {project && (
            <Suspense fallback={null}>
              <div
                style={{
                  flex: 1,
                  overflow: "auto",
                  padding: 32,
                  display: projectView === "overview" ? "block" : "none",
                }}
              >
                <OverviewView />
              </div>
              <div
                style={{
                  flex: 1,
                  display: projectView === "canvas" ? "flex" : "none",
                  flexDirection: "column",
                  overflow: "hidden",
                  minHeight: 0,
                }}
              >
                <CanvasView isActive={projectView === "canvas"} />
              </div>
              <div
                style={{
                  flex: 1,
                  display: projectView === "editor" ? "flex" : "none",
                  overflow: "hidden",
                  minHeight: 0,
                }}
              >
                {EditorView ? <EditorView /> : <NetworkEditor />}
              </div>
              <div
                style={{
                  flex: 1,
                  display: projectView === "analysis" ? "flex" : "none",
                  flexDirection: "column",
                  overflow: "hidden",
                  minHeight: 0,
                }}
              >
                {EngineAnalysisView ? <EngineAnalysisView /> : <AnalysisView />}
              </div>
              <div
                style={{
                  flex: 1,
                  display: projectView === "report" ? "flex" : "none",
                  flexDirection: "column",
                  overflow: "hidden",
                  minHeight: 0,
                }}
              >
                <ReportView />
              </div>
            </Suspense>
          )}
        </div>
      </div>
    </ProjectPeriodProvider>
  );
}
