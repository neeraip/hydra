import { useActiveProject, useAppState } from "../../AppContext";
import { SimulationSettings } from "../../components/editors/SimulationSettings";
import {
  LABEL,
  openBaseFolder,
  useLinks,
  useNodes,
  useScenarios,
} from "../../hooks";
import { Header } from "./OverviewView/Header";
import { NetworkComposition } from "./OverviewView/NetworkComposition";
import { ProjectInfo } from "./OverviewView/ProjectInfo";
import { Section } from "./OverviewView/primitives";
import { ScenarioList } from "./OverviewView/ScenarioList";

// ─────────────────────────────────────────────────────────────────────────────
// Project overview
//
// Goal: give an engineer, in a single glance, a sense of (1) what's in the
// model, (2) how the last simulation behaved, and (3) what scenarios exist —
// driven entirely from real data when available, with quiet empty states
// otherwise. Nothing fabricated, nothing decorative-only.
// ─────────────────────────────────────────────────────────────────────────────

export function OverviewView() {
  const { setProjectView, openScenariosModal, openCrsModal, scenariosVersion } =
    useAppState();
  const { project, accent } = useActiveProject();

  const nodes = useNodes();
  const links = useLinks();
  const scenarios = useScenarios(project?.id ?? null, scenariosVersion);

  if (!project) {
    return (
      <div style={{ color: "var(--text-tertiary)", fontSize: 14, padding: 32 }}>
        No project selected.
      </div>
    );
  }

  const networkLoaded = nodes.length > 0 || links.length > 0;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 20,
        paddingBottom: 24,
      }}
    >
      <Header
        project={project}
        accent={accent}
        onOpenCanvas={() => setProjectView("canvas")}
        onOpenEditor={() => setProjectView("editor")}
        onOpenAnalysis={() => setProjectView("analysis")}
        onOpenFolder={() => openBaseFolder(project.id)}
      />

      {/* ── Tier 1: Static project facts ─────────────────────────────── */}
      <Section title="Network">
        <NetworkComposition
          nodes={nodes}
          links={links}
          networkLoaded={networkLoaded}
          fallbackNodeCount={project.nodeCount}
          fallbackLinkCount={project.linkCount}
        />
      </Section>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "minmax(0, 1.4fr) minmax(0, 1fr)",
          gap: 16,
        }}
      >
        <Section title="Scenarios" right={`${scenarios.length}`}>
          <ScenarioList
            scenarios={scenarios}
            onOpen={() => openScenariosModal()}
          />
        </Section>
        <Section title="Project info">
          <ProjectInfo
            crs={project.sourceCrs}
            modifiedLabel={project.modifiedLabel}
            lastRunLabel={project.lastRunLabel ?? null}
            engineLabel={LABEL}
            onEditCrs={openCrsModal}
            onOpenFolder={() => openBaseFolder(project.id)}
          />
        </Section>
      </div>

      {/* ── Tier 2: Simulation configuration ─────────────────────────── */}
      <Section title="Simulation settings">
        <SimulationSettings projectId={project.id} />
      </Section>
    </div>
  );
}
