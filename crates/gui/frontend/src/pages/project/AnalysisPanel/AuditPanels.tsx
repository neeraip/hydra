import { useSimulation } from "../../../AppContext";
import { SectionHeader } from "../../../components/ui/SectionHeader";
import type { ResultAnalytics } from "../../../hooks";
import { formatQty, useUnitSystem } from "../../../units";
import { AuditMetric, NoDataCard, Sparkline } from "./charts";

export function AuditPanels({
  analytics,
  periodCount,
}: {
  analytics: ResultAnalytics | null;
  periodCount: number | null;
}) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20 }}>
      <MassBalanceAudit analytics={analytics} />
      <EnergyAudit periodCount={periodCount} />
    </div>
  );
}

function MassBalanceAudit({
  analytics,
}: {
  analytics: ResultAnalytics | null;
}) {
  const sys = useUnitSystem();
  if (!analytics) {
    return (
      <div>
        <SectionHeader>Mass-balance audit</SectionHeader>
        <NoDataCard message="Run a simulation to see the mass-balance audit." />
      </div>
    );
  }
  const { massBalance, periodCount } = analytics;
  const mbMin = Math.min(99, ...massBalance.series);
  return (
    <div>
      <SectionHeader>Mass-balance audit</SectionHeader>
      <div
        style={{
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: 16,
        }}
      >
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
            gap: 8,
            marginBottom: 14,
          }}
        >
          <AuditMetric
            value={formatQty(massBalance.inflowM3, "volume", sys, 0)}
            label="Cumulative inflow"
          />
          <AuditMetric
            value={formatQty(massBalance.outflowM3, "volume", sys, 0)}
            label="Cumulative outflow"
          />
          <AuditMetric value={`${periodCount} steps`} label="Timesteps" />
          <AuditMetric
            value={`${massBalance.balancePct.toFixed(2)} %`}
            label="Mass balance"
            valueColor="var(--status-success)"
          />
        </div>
        {massBalance.series.length > 1 && (
          <>
            <div
              style={{
                fontSize: 11,
                color: "var(--text-tertiary)",
                marginBottom: 6,
              }}
            >
              Mass balance % over simulation horizon
            </div>
            <Sparkline
              values={massBalance.series}
              min={Math.max(0, mbMin - 0.5)}
              max={100}
              stroke="var(--status-success)"
            />
          </>
        )}
      </div>
    </div>
  );
}

function EnergyAudit({ periodCount }: { periodCount: number | null }) {
  const sys = useUnitSystem();
  const { pumpEnergy, resultMeta } = useSimulation();
  if (!pumpEnergy) {
    return (
      <div>
        <SectionHeader>Energy audit</SectionHeader>
        <NoDataCard message="Run a simulation to see pump energy data." />
      </div>
    );
  }
  // Real reporting-period duration in hours, derived from snapshot-time
  // spacing (seconds). Falls back to 1 h when times are unavailable.
  const times = resultMeta?.times;
  const hoursPerPeriod =
    times && times.length > 1
      ? (times[times.length - 1] - times[0]) / (times.length - 1) / 3600
      : 1;
  const totalKwh = pumpEnergy.reduce(
    (s, p) =>
      s + p.avgKw * (p.pctOnline / 100) * (periodCount ?? 1) * hoursPerPeriod,
    0,
  );
  const peakKw = pumpEnergy.reduce((s, p) => Math.max(s, p.peakKw), 0);
  const pumpsWithFlow = pumpEnergy.filter((p) => p.avgKwhPerFlow > 0);
  const specificEnergy =
    pumpsWithFlow.length > 0
      ? pumpsWithFlow.reduce((s, p) => s + p.avgKwhPerFlow, 0) /
        pumpsWithFlow.length
      : null;
  return (
    <div>
      <SectionHeader>Energy audit</SectionHeader>
      <div
        style={{
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: 16,
        }}
      >
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
            gap: 8,
          }}
        >
          <AuditMetric
            value={`${periodCount ?? "—"} steps`}
            label="Timesteps computed"
          />
          <AuditMetric
            value={pumpEnergy.length > 0 ? `${totalKwh.toFixed(1)} kWh` : "—"}
            label="Total pump energy"
          />
          <AuditMetric
            value={
              specificEnergy != null
                ? sys === "us"
                  ? // kWh/m³ → kWh per 1000 US gal (customary specific energy).
                    `${((specificEnergy / 264.172) * 1000).toFixed(3)} kWh/kgal`
                  : `${specificEnergy.toFixed(3)} kWh/m³`
                : "—"
            }
            label="Specific energy"
          />
          <AuditMetric
            value={pumpEnergy.length > 0 ? `${peakKw.toFixed(1)} kW` : "—"}
            label="Peak power"
          />
        </div>
        {pumpEnergy.length === 0 && (
          <div
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              marginTop: 12,
              lineHeight: 1.4,
            }}
          >
            No pump energy data. The network may have no pumps.
          </div>
        )}
      </div>
    </div>
  );
}
