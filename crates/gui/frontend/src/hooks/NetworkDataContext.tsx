import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { perfTrace } from "../perfTrace";
import type { Link, Node } from "../types";
import { tryInvoke } from "./ipc";
import { useNetworkVersion } from "./NetworkVersionContext";

export interface NetworkSummary {
  totalNodes: number;
  totalLinks: number;
  junctions: number;
  tanks: number;
  reservoirs: number;
  pipes: number;
  pumps: number;
  valves: number;
  totalLengthM: number;
  meanDiaMm: number | null;
  totalPumpKw: number | null;
}

interface NetworkDataCtx {
  nodes: Node[];
  links: Link[];
  summary: NetworkSummary;
  loading: boolean;
  primeNetworkData: (snapshot: NetworkSnapshotDto) => void;
}

interface NetworkSnapshotDto {
  nodes: Node[];
  links: Link[];
}

function summarizeNetwork(nodes: Node[], links: Link[]): NetworkSummary {
  let junctions = 0;
  let tanks = 0;
  let reservoirs = 0;
  for (const n of nodes) {
    if (n.type === "junction") junctions += 1;
    else if (n.type === "tank") tanks += 1;
    else if (n.type === "reservoir") reservoirs += 1;
  }

  let pipes = 0;
  let pumps = 0;
  let valves = 0;
  let totalLengthM = 0;
  let diaSum = 0;
  let diaCount = 0;
  let totalPumpKw = 0;
  let pumpKwCount = 0;

  for (const l of links) {
    if (l.type === "pipe") {
      pipes += 1;
      if (typeof l.length === "number" && l.length > 0)
        totalLengthM += l.length;
      if (l.diameter > 0) {
        diaSum += l.diameter;
        diaCount += 1;
      }
      continue;
    }
    if (l.type === "pump") {
      pumps += 1;
      if (typeof l.pumpPowerKw === "number") {
        totalPumpKw += l.pumpPowerKw;
        pumpKwCount += 1;
      }
      continue;
    }
    if (l.type === "valve") valves += 1;
  }

  return {
    totalNodes: nodes.length,
    totalLinks: links.length,
    junctions,
    tanks,
    reservoirs,
    pipes,
    pumps,
    valves,
    totalLengthM,
    meanDiaMm: diaCount > 0 ? diaSum / diaCount : null,
    totalPumpKw: pumpKwCount > 0 ? totalPumpKw : null,
  };
}

const EMPTY_SUMMARY: NetworkSummary = {
  totalNodes: 0,
  totalLinks: 0,
  junctions: 0,
  tanks: 0,
  reservoirs: 0,
  pipes: 0,
  pumps: 0,
  valves: 0,
  totalLengthM: 0,
  meanDiaMm: null,
  totalPumpKw: null,
};

const Ctx = createContext<NetworkDataCtx>({
  nodes: [],
  links: [],
  summary: EMPTY_SUMMARY,
  loading: false,
  primeNetworkData: () => {},
});

export function NetworkDataProvider({ children }: { children: ReactNode }) {
  const { version } = useNetworkVersion();
  const [nodes, setNodes] = useState<Node[]>([]);
  const [links, setLinks] = useState<Link[]>([]);
  const [loading, setLoading] = useState(false);
  const skipNextFetchRef = useRef(false);

  const primeNetworkData = useCallback((snapshot: NetworkSnapshotDto) => {
    skipNextFetchRef.current = true;
    setNodes(snapshot.nodes);
    setLinks(snapshot.links);
    setLoading(false);
  }, []);

  useEffect(() => {
    if (skipNextFetchRef.current) {
      skipNextFetchRef.current = false;
      setLoading(false);
      return;
    }

    let cancelled = false;
    setLoading(true);
    const fetchStartedAt = performance.now();

    tryInvoke<NetworkSnapshotDto>("get_network_snapshot")
      .then((snapshot) => {
        if (cancelled) return;
        const nextNodes = snapshot?.nodes ?? [];
        const nextLinks = snapshot?.links ?? [];
        const nodeCount = nextNodes.length;
        const linkCount = nextLinks.length;
        setNodes(nextNodes);
        setLinks(nextLinks);
        if (nodeCount > 0 || linkCount > 0) {
          perfTrace("network-data-fetch", performance.now() - fetchStartedAt, {
            version,
            nodeCount,
            linkCount,
          });
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [version]);

  const summary = useMemo(() => summarizeNetwork(nodes, links), [links, nodes]);

  const value = useMemo(
    () => ({ nodes, links, summary, loading, primeNetworkData }),
    [links, loading, nodes, primeNetworkData, summary],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useNetworkData() {
  return useContext(Ctx);
}
