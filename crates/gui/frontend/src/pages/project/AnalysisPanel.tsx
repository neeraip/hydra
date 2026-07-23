import { useEffect, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { MetricChip } from "../../components/ui/MetricChip";
import { WarningRow } from "../../components/ui/WarningRow";
import {
  getResultAnalytics,
  PRESSURE_THRESHOLD,
  type PumpEnergyRecord,
  type ResultAnalytics,
} from "../../hooks";
import { formatQty, useUnitSystem } from "../../units";
import { AuditPanels } from "./AnalysisPanel/AuditPanels";
import { pressureCompliancePct } from "./AnalysisPanel/compliance";
import {
  PressureHistogram,
  VelocityHistogram,
} from "./AnalysisPanel/Histograms";
import { PipeCriticality } from "./AnalysisPanel/PipeCriticality";
import { PumpEnergyPanel } from "./AnalysisPanel/PumpEnergyPanel";
import { TankLevelsPanel } from "./AnalysisPanel/TankLevelsPanel";

export function AnalysisPanel() {
  const { resultMeta, pumpEnergy } = useSimulation();
  const { project } = useActiveProject();
  const { activeScenarioId, deferredProjectView } = useAppState();
  const visible = deferredProjectView === "analysis";

  // Load analytics from the backend — streams the .out file one period at a
  // time so it is safe for arbitrarily large networks.  Re-fetches whenever
  // the result changes (resultMeta changes on every new run).
  const [analytics, setAnalytics] = useState<ResultAnalytics | null>(null);
  useEffect(() => {
    if (!project?.id || !resultMeta) {
      setAnalytics(null);
      return;
    }
    // Gated on visibility: the panel stays mounted while hidden, and this
    // fetch streams the whole .out file server-side — running it during a
    // tab/scenario switch contended with the switch's own IPC. On becoming
    // visible the effect re-runs and fetches (analytics may be one result
    // behind while hidden, which is fine — nothing displays it).
    if (!visible) return;
    let cancelled = false;
    getResultAnalytics(project.id, activeScenarioId)
      .then((a) => {
        if (!cancelled) setAnalytics(a);
      })
      .catch((err) => {
        // Fall back to the empty-state placeholders ("—" metrics).
        console.error("Failed to load result analytics:", err);
        if (!cancelled) setAnalytics(null);
      });
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, resultMeta, visible]);

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
      <SystemSummary analytics={analytics} pumpEnergy={pumpEnergy} />

      {/* Panel 2: Two-column histograms */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20 }}>
        <PressureHistogram analytics={analytics} />
        <VelocityHistogram analytics={analytics} />
      </div>

      {/* Panel 3: Top pipes by max velocity */}
      <PipeCriticality analytics={analytics} />

      {/* Panel 4: Mass-balance & energy audit */}
      <AuditPanels
        analytics={analytics}
        periodCount={analytics?.periodCount ?? null}
      />

      {/* Panel 5: Tank head trends */}
      <TankLevelsPanel analytics={analytics} />

      {/* Panel 6: Pump Energy */}
      <PumpEnergyPanel />
    </div>
  );
}

/* ── System Summary ──────────────────────────────────────────────────────────── */

/**
 * Total pump energy over the run as a chip value, or "—" when pump energy
 * hasn't loaded (or predates the backend's `totalKwh` field).
 */
function pumpEnergyChipValue(pumpEnergy: PumpEnergyRecord[] | null): string {
  if (!pumpEnergy || pumpEnergy.length === 0) return "—";
  let total = 0;
  let hasKwh = false;
  for (const p of pumpEnergy) {
    if (typeof p.totalKwh === "number" && Number.isFinite(p.totalKwh)) {
      total += p.totalKwh;
      hasKwh = true;
    }
  }
  if (!hasKwh) return "—";
  return `${total >= 100 ? total.toFixed(0) : total.toFixed(1)} kWh`;
}

function SystemSummary({
  analytics,
  pumpEnergy,
}: {
  analytics: ResultAnalytics | null;
  pumpEnergy: PumpEnergyRecord[] | null;
}) {
  const sys = useUnitSystem();
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

  const compliancePct = pressureCompliancePct(analytics);

  // Min-pressure / max-velocity markers are absent when no valid data exists
  // in the results (e.g. no junctions, or no links with velocity data).
  const hasMinPressure = analytics.minPressureM != null;
  const minPressureColor =
    analytics.minPressureM != null &&
    analytics.minPressureM < PRESSURE_THRESHOLD
      ? "var(--status-error)"
      : undefined;

  return (
    <div>
      <div style={{ display: "flex", gap: 12, marginBottom: 12 }}>
        <MetricChip
          value={
            analytics.minPressureM != null
              ? formatQty(analytics.minPressureM, "pressure", sys, 1)
              : "—"
          }
          label={
            hasMinPressure && analytics.minPressureNodeId != null
              ? `Min Pressure (${analytics.minPressureNodeId})`
              : "Min Pressure"
          }
          valueColor={minPressureColor}
        />
        <MetricChip
          value={
            analytics.maxVelocityMs != null
              ? formatQty(analytics.maxVelocityMs, "velocity", sys, 2)
              : "—"
          }
          label={
            analytics.maxVelocityMs != null &&
            analytics.maxVelocityLinkId != null
              ? `Max Velocity (${analytics.maxVelocityLinkId})`
              : "Max Velocity"
          }
        />
        {compliancePct != null && (
          <MetricChip
            value={`${compliancePct.toFixed(1)} %`}
            label={`Pressure ≥ ${formatQty(PRESSURE_THRESHOLD, "pressure", sys, sys === "si" ? 0 : 1)}`}
            valueColor={
              compliancePct < 100 ? "var(--status-warning)" : undefined
            }
          />
        )}
        <MetricChip
          value={pumpEnergyChipValue(pumpEnergy)}
          label="Pump Energy"
        />
        <MetricChip
          value={`${analytics.massBalance.balancePct.toFixed(1)} %`}
          label="Mass Balance"
        />
      </div>
      {analytics.lowPressureCount > 0 && (
        <WarningRow>
          {analytics.lowPressureCount} junction
          {analytics.lowPressureCount > 1 ? "s" : ""} below the minimum pressure
          threshold of{" "}
          {formatQty(PRESSURE_THRESHOLD, "pressure", sys, sys === "si" ? 0 : 1)}{" "}
          at peak demand.
        </WarningRow>
      )}
    </div>
  );
}
