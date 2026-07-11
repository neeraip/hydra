import { lazy, Suspense } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { ScenarioStrip } from "../../components/layout/ScenarioStrip";
import { SecondaryRail } from "../../components/layout/SecondaryRail";

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

export function ProjectPage() {
  const { projectView } = useAppState();
  const { project } = useActiveProject();

  return (
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
      <ScenarioStrip />
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
              <NetworkEditor />
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
              <AnalysisView />
            </div>
          </Suspense>
        )}
      </div>
    </div>
  );
}
