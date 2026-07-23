import { useSimulation } from "../../../AppContext";
import { SectionHeader } from "../../../components/ui/SectionHeader";
import { NoDataCard } from "./charts";

export function PumpEnergyPanel() {
  const { pumpEnergy } = useSimulation();

  if (!pumpEnergy || pumpEnergy.length === 0) {
    return (
      <div className="insights-card">
        <SectionHeader>Pump Energy</SectionHeader>
        <NoDataCard
          message={
            pumpEnergy === null
              ? "Run a simulation to see pump energy data."
              : "No pump links detected in the simulation results."
          }
        />
      </div>
    );
  }

  const pumpRows = pumpEnergy
    .map((p) => ({ id: p.id, energy: p.avgKw, unit: "kW avg" }))
    .filter((r) => r.energy > 0)
    .sort((a, b) => b.energy - a.energy);

  if (pumpRows.length === 0) {
    return (
      <div className="insights-card">
        <SectionHeader>Pump Energy</SectionHeader>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          All pumps are offline or have zero power.
        </div>
      </div>
    );
  }

  const maxEnergy = Math.max(...pumpRows.map((r) => r.energy));

  // Whole-run totals. `totalKwh` may be absent at runtime while the backend
  // predates the field — the row is shown only when real totals exist.
  let totalKwh = 0;
  let hasKwh = false;
  let totalCost = 0;
  let hasCost = false;
  for (const p of pumpEnergy) {
    if (typeof p.totalKwh === "number" && Number.isFinite(p.totalKwh)) {
      totalKwh += p.totalKwh;
      hasKwh = true;
    }
    if (typeof p.totalCost === "number" && Number.isFinite(p.totalCost)) {
      totalCost += p.totalCost;
      hasCost = true;
    }
  }

  return (
    <div className="insights-card">
      <SectionHeader>Pump Energy</SectionHeader>
      {pumpRows.map((row) => {
        const pct = (row.energy / maxEnergy) * 100;
        return (
          <div
            key={row.id}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 12,
              marginBottom: 8,
            }}
          >
            <span
              style={{
                fontSize: 12,
                color: "var(--text-secondary)",
                minWidth: 48,
              }}
            >
              {row.id}
            </span>
            <div
              style={{
                flex: 1,
                height: 10,
                background: "var(--border)",
                borderRadius: 5,
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  width: `${pct}%`,
                  height: "100%",
                  background:
                    "linear-gradient(90deg, var(--accent) 0%, rgba(74,144,217,0.6) 100%)",
                  borderRadius: 5,
                }}
              />
            </div>
            <span
              style={{
                fontSize: 12,
                fontFamily: "var(--font-mono)",
                color: "var(--text-primary)",
                minWidth: 80,
                textAlign: "right",
              }}
            >
              {row.energy.toFixed(1)} {row.unit}
            </span>
          </div>
        );
      })}
      {hasKwh && (
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            gap: 12,
            marginTop: 10,
            paddingTop: 10,
            borderTop: "1px solid var(--border)",
          }}
        >
          <span
            style={{
              fontSize: 12,
              fontWeight: 600,
              color: "var(--text-secondary)",
            }}
          >
            Total
          </span>
          <span
            style={{
              fontSize: 12,
              fontFamily: "var(--font-mono)",
              color: "var(--text-primary)",
              textAlign: "right",
            }}
          >
            {totalKwh.toFixed(1)} kWh
            {hasCost && (
              <span style={{ color: "var(--text-secondary)" }}>
                {" "}
                · {totalCost.toFixed(2)} energy cost
              </span>
            )}
          </span>
        </div>
      )}
    </div>
  );
}
