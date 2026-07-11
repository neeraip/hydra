import type { NetworkSummary } from "../../../hooks";
import { Kpi, KpiGrid } from "./primitives";

export function NetworkComposition({
  summary,
  networkLoaded,
  fallbackNodeCount,
  fallbackLinkCount,
}: {
  summary: NetworkSummary;
  networkLoaded: boolean;
  fallbackNodeCount: number;
  fallbackLinkCount: number;
}) {
  if (!networkLoaded) {
    return (
      <KpiGrid>
        <Kpi
          label="Nodes"
          value={fallbackNodeCount.toLocaleString()}
          sub="loading details…"
          muted
        />
        <Kpi
          label="Links"
          value={fallbackLinkCount.toLocaleString()}
          sub="loading details…"
          muted
        />
        <Kpi label="Pumps" value="—" sub="—" muted />
        <Kpi label="Storage" value="—" sub="—" muted />
      </KpiGrid>
    );
  }

  const lengthLabel =
    summary.totalLengthM >= 10000
      ? `${(summary.totalLengthM / 1000).toFixed(1)} km total`
      : `${Math.round(summary.totalLengthM).toLocaleString()} m total`;
  const diaLabel =
    summary.meanDiaMm !== null
      ? `Ø ${Math.round(summary.meanDiaMm)} mm avg`
      : "Ø —";
  const storageValue = `${summary.tanks + summary.reservoirs}`;
  const storageSub =
    summary.tanks + summary.reservoirs === 0
      ? "no tanks or reservoirs"
      : `${summary.tanks} tank${summary.tanks === 1 ? "" : "s"} · ${summary.reservoirs} reservoir${summary.reservoirs === 1 ? "" : "s"}`;
  const pumpsSub =
    summary.pumps === 0
      ? summary.valves === 0
        ? "no pumps or valves"
        : `${summary.valves} valve${summary.valves === 1 ? "" : "s"}`
      : summary.totalPumpKw !== null
        ? `${summary.totalPumpKw.toFixed(0)} kW total rated`
        : `${summary.valves} valve${summary.valves === 1 ? "" : "s"} also`;

  return (
    <KpiGrid>
      <Kpi
        label="Junctions"
        value={summary.junctions.toLocaleString()}
        sub={`${summary.tanks + summary.reservoirs} other node${summary.tanks + summary.reservoirs === 1 ? "" : "s"}`}
      />
      <Kpi
        label="Pipes"
        value={summary.pipes.toLocaleString()}
        sub={`${lengthLabel} · ${diaLabel}`}
      />
      <Kpi
        label="Pumps"
        value={summary.pumps.toLocaleString()}
        sub={pumpsSub}
      />
      <Kpi label="Storage" value={storageValue} sub={storageSub} />
    </KpiGrid>
  );
}
