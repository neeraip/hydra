import { useEffect, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { MetricChip } from "../../components/ui/MetricChip";
import { WarningRow } from "../../components/ui/WarningRow";
import {
  getResultAnalytics,
  PRESSURE_THRESHOLD,
  type ResultAnalytics,
} from "../../hooks";
import { AuditPanels } from "./AnalysisPanel/AuditPanels";
import {
  PressureHistogram,
  VelocityHistogram,
} from "./AnalysisPanel/Histograms";
import { PipeCriticality } from "./AnalysisPanel/PipeCriticality";
import { PumpEnergyPanel } from "./AnalysisPanel/PumpEnergyPanel";

export function AnalysisPanel() {
  const { resultMeta } = useSimulation();
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();

  // Load analytics from the backend — streams the .out file one period at a
  // time so it is safe for arbitrarily large networks.  Re-fetches whenever
  // the result changes (resultMeta changes on every new run).
  const [analytics, setAnalytics] = useState<ResultAnalytics | null>(null);
  useEffect(() => {
    if (!project?.id || !resultMeta) {
      setAnalytics(null);
      return;
    }
    let cancelled = false;
    getResultAnalytics(project.id, activeScenarioId).then((a) => {
      if (!cancelled) setAnalytics(a);
    });
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, resultMeta]);

  return (
    <div
      style={{
        padding: 24,
        display: "flex",
        flexDirection: "column",
        gap: 20,
        animation: "fadeIn 150ms ease-out",
      }}
    >
      {/* Panel 1: System Summary */}
      <SystemSummary analytics={analytics} />

      {/* Panel 2: Two-column histograms */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20 }}>
        <PressureHistogram analytics={analytics} />
        <VelocityHistogram analytics={analytics} />
      </div>

      {/* Panel 3: Pipe Criticality */}
      <PipeCriticality analytics={analytics} />

      {/* Panel 4: Mass-balance & energy audit */}
      <AuditPanels
        analytics={analytics}
        periodCount={analytics?.periodCount ?? null}
      />

      {/* Panel 5: Pump Energy */}
      <PumpEnergyPanel />
    </div>
  );
}

/* ── System Summary ──────────────────────────────────────────────────────────── */

function SystemSummary({ analytics }: { analytics: ResultAnalytics | null }) {
  if (!analytics) {
    return (
      <div>
        <div style={{ display: "flex", gap: 12, marginBottom: 12 }}>
          <MetricChip value="—" label="Min Pressure" />
          <MetricChip value="—" label="Max Velocity" />
          <MetricChip value="—" label="Pump Energy" />
          <MetricChip value="—" label="Mass Balance" />
        </div>
        <WarningRow>Run a simulation to see real system metrics.</WarningRow>
      </div>
    );
  }

  const minPressureColor =
    analytics.minPressureM < PRESSURE_THRESHOLD
      ? "var(--status-error)"
      : undefined;

  return (
    <div>
      <div style={{ display: "flex", gap: 12, marginBottom: 12 }}>
        <MetricChip
          value={`${analytics.minPressureM.toFixed(1)} m`}
          label={`Min Pressure (${analytics.minPressureNodeId})`}
          valueColor={minPressureColor}
        />
        <MetricChip
          value={`${analytics.maxVelocityMs.toFixed(2)} m/s`}
          label={`Max Velocity (${analytics.maxVelocityLinkId})`}
        />
        <MetricChip value="—" label="Pump Energy" />
        <MetricChip
          value={`${analytics.massBalance.balancePct.toFixed(1)} %`}
          label="Mass Balance"
        />
      </div>
      {analytics.lowPressureCount > 0 && (
        <WarningRow>
          {analytics.lowPressureCount} junction
          {analytics.lowPressureCount > 1 ? "s" : ""} below the minimum pressure
          threshold of {PRESSURE_THRESHOLD} m at peak demand.
        </WarningRow>
      )}
    </div>
  );
}
