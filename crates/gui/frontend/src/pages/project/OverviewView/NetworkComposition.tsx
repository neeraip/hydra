import { useMemo } from "react";
import type { useLinks, useNodes } from "../../../hooks";
import { Kpi, KpiGrid } from "./primitives";

export function NetworkComposition({
  nodes,
  links,
  networkLoaded,
  fallbackNodeCount,
  fallbackLinkCount,
}: {
  nodes: ReturnType<typeof useNodes>;
  links: ReturnType<typeof useLinks>;
  networkLoaded: boolean;
  fallbackNodeCount: number;
  fallbackLinkCount: number;
}) {
  const stats = useMemo(() => {
    const junctions = nodes.filter((n) => n.type === "junction").length;
    const tanks = nodes.filter((n) => n.type === "tank").length;
    const reservoirs = nodes.filter((n) => n.type === "reservoir").length;
    const pipes = links.filter((l) => l.type === "pipe");
    const pumps = links.filter((l) => l.type === "pump");
    const valves = links.filter((l) => l.type === "valve");

    let totalLengthM = 0;
    let diaSum = 0;
    let diaCount = 0;
    for (const p of pipes) {
      if (typeof p.length === "number" && p.length > 0)
        totalLengthM += p.length;
      if (p.diameter > 0) {
        diaSum += p.diameter;
        diaCount += 1;
      }
    }
    const meanDiaMm = diaCount > 0 ? diaSum / diaCount : null;

    let totalPumpKw = 0;
    let pumpKwCount = 0;
    for (const pu of pumps) {
      if (typeof pu.pumpPowerKw === "number") {
        totalPumpKw += pu.pumpPowerKw;
        pumpKwCount += 1;
      }
    }

    return {
      junctions,
      tanks,
      reservoirs,
      pipes: pipes.length,
      pumps: pumps.length,
      valves: valves.length,
      totalLengthM,
      meanDiaMm,
      totalPumpKw: pumpKwCount > 0 ? totalPumpKw : null,
    };
  }, [nodes, links]);

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
    stats.totalLengthM >= 10000
      ? `${(stats.totalLengthM / 1000).toFixed(1)} km total`
      : `${Math.round(stats.totalLengthM).toLocaleString()} m total`;
  const diaLabel =
    stats.meanDiaMm !== null
      ? `Ø ${Math.round(stats.meanDiaMm)} mm avg`
      : "Ø —";
  const storageValue = `${stats.tanks + stats.reservoirs}`;
  const storageSub =
    stats.tanks + stats.reservoirs === 0
      ? "no tanks or reservoirs"
      : `${stats.tanks} tank${stats.tanks === 1 ? "" : "s"} · ${stats.reservoirs} reservoir${stats.reservoirs === 1 ? "" : "s"}`;
  const pumpsSub =
    stats.pumps === 0
      ? stats.valves === 0
        ? "no pumps or valves"
        : `${stats.valves} valve${stats.valves === 1 ? "" : "s"}`
      : stats.totalPumpKw !== null
        ? `${stats.totalPumpKw.toFixed(0)} kW total rated`
        : `${stats.valves} valve${stats.valves === 1 ? "" : "s"} also`;

  return (
    <KpiGrid>
      <Kpi
        label="Junctions"
        value={stats.junctions.toLocaleString()}
        sub={`${stats.tanks + stats.reservoirs} other node${stats.tanks + stats.reservoirs === 1 ? "" : "s"}`}
      />
      <Kpi
        label="Pipes"
        value={stats.pipes.toLocaleString()}
        sub={`${lengthLabel} · ${diaLabel}`}
      />
      <Kpi label="Pumps" value={stats.pumps.toLocaleString()} sub={pumpsSub} />
      <Kpi label="Storage" value={storageValue} sub={storageSub} />
    </KpiGrid>
  );
}
