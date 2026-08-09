import { lazy, Suspense, useEffect, useRef } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { ProjectPeriodProvider } from "../../canvas/period-context";
import { ProjectToolbar } from "../../components/layout/ProjectToolbar";
import { SecondaryRail } from "../../components/layout/SecondaryRail";
import { engineComponents } from "../../engine/registry";
import { useModelUnitSystem } from "../../hooks";
import { startPerfSpan } from "../../perfTrace";
import {
  ResolvedUnitSystem,
  resolveUnitSystem,
  type UnitPreference,
  useUnitPreference,
} from "../../units";

const OverviewView = lazy(() =>
  import("./OverviewView").then((m) => ({ default: m.OverviewView })),
);
const CanvasView = lazy(() =>
  import("./CanvasView").then((m) => ({ default: m.CanvasView })),
);
const NetworkEditor = lazy(() =>
  import("./NetworkEditor").then((m) => ({ default: m.NetworkEditor })),
);
const AnalysisPanel = lazy(() =>
  import("./AnalysisPanel").then((m) => ({ default: m.AnalysisPanel })),
);
const ReportView = lazy(() =>
  import("./ReportView").then((m) => ({ default: m.ReportView })),
);

export function ProjectPage() {
  // Deferred: the tab highlight (TopBar) reads the urgent value; the heavy
  // view subtrees flip one interruptible render later so the click paints
  // instantly even on 46k-element networks.
  const { deferredProjectView: projectView, activeScenarioId } = useAppState();
  const { project, engine } = useActiveProject();

  // Everything below this point reads units through `useUnitSystem`, which
  // needs the *resolved* system — and resolving it needs the active
  // project. Hence a provider here rather than in the module store: only
  // this subtree has a model to follow.
  const appDefault = useUnitPreference();
  const modelUnits = useModelUnitSystem(project?.id, activeScenarioId);
  const resolvedUnits = resolveUnitSystem(
    project?.unitSystem as UnitPreference | undefined,
    appDefault,
    modelUnits,
  );
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
    <ResolvedUnitSystem.Provider value={resolvedUnits}>
      <ProjectPeriodProvider>
        {/* The engine's colour reaches the primary button through this
            subtree. Setting it is opt-in and the fallback is achromatic, so
            a surface says whether it belongs to an engine rather than
            inheriting an answer from where it happens to be mounted.

            Position in the tree is not that question, which is worth
            stating because this comment first claimed it was. The Settings
            drawer is a sibling of this element and should be achromatic —
            true. The run modal is also a sibling and should *not* be: it
            runs one engine's model and draws that engine's badge. It sets
            the variable itself, as does the simulation settings modal.

            Both halves travel together: a fill and what can be read on it.
            The engine accents are mid-tone by design and carry white; the
            achromatic accent is near-white in the dark theme and cannot,
            which is what `--accent-fg` is for. */}
        <div
          style={
            {
              flex: 1,
              height: "100%",
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              animation: "fadeIn 150ms ease-out",
              ...(engine?.accent
                ? {
                    "--engine-accent": engine.accent,
                    "--engine-accent-fg": "#fff",
                  }
                : null),
            } as React.CSSProperties
          }
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
                  {EngineAnalysisView ? (
                    <EngineAnalysisView />
                  ) : (
                    <AnalysisPanel />
                  )}
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
    </ResolvedUnitSystem.Provider>
  );
}
