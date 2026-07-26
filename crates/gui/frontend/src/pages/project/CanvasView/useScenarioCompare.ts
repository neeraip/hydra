import {
  type Dispatch,
  type SetStateAction,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useAppState } from "../../../AppContext";
import { type CompareDeltas, computeDeltas } from "../../../canvas/compare";
import type { LinkVariable, NodeVariable } from "../../../canvas/types";
import {
  compareTopologyDigests,
  getPeriodResults,
  loadResultMeta,
  type PeriodResults,
  type ResultMeta,
  useScenarios,
} from "../../../hooks";
import { useSimulation } from "../../../SimulationContext";

/** Sentinel baseline id meaning "compare against the base model" (only
 * meaningful while a scenario is active). Distinct from `null` = off. */
export const BASE_COMPARE_ID = "__base__";

/**
 * Scenario-comparison (Δ overlay) pipeline for the canvas.
 *
 * The user picks a baseline; the canvas colours by (active − baseline) for
 * the selected variables on a diverging ramp centred at zero. Owns: baseline
 * validity resolution (stale persisted ids are inert, never crashing),
 * baseline result-metadata fetching (cached per project/baseline/generation),
 * per-period baseline fetches that follow the timeline scrub, the
 * topology-staleness gate, delta computation, the "why comparison can't run"
 * notice, the Legend's Δ caption/ranges, and the baseline picker options.
 */
export function useScenarioCompare({
  projectId,
  compareScenarioId,
  currentPeriodResult,
  currentHour,
  nodeCount,
  linkCount,
  nodeVar,
  linkVar,
}: {
  projectId: string | null;
  /** Persisted picker selection: null = off, BASE_COMPARE_ID, or scenario id. */
  compareScenarioId: string | null;
  /** The ACTIVE scenario's current-period arrays (already staleness-gated). */
  currentPeriodResult: PeriodResults | null;
  currentHour: number;
  nodeCount: number;
  linkCount: number;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
}): {
  effectiveCompareId: string | null;
  comparing: boolean;
  baselineName: string;
  compareDeltas: CompareDeltas | null;
  compareNotice: string | null;
  compareNoticeDismissed: boolean;
  setCompareNoticeDismissed: Dispatch<SetStateAction<boolean>>;
  legendCompare: {
    baselineName: string;
    nodeMaxAbs: number;
    linkMaxAbs: number | null;
  } | null;
  compareOptions: { value: string | null; label: string }[];
} {
  const { activeScenarioId } = useAppState();
  const { resultMeta, resultGeneration, liveNetworkDigest } = useSimulation();
  const scenarios = useScenarios(projectId);

  // Resolve the persisted selection against current reality: "base model" is
  // only a valid baseline while a scenario is active, a scenario baseline
  // must exist and must not be the active scenario itself. Invalid selections
  // are inert (treated as off) rather than destructively reset.
  const effectiveCompareId = useMemo(() => {
    if (!compareScenarioId) return null;
    if (compareScenarioId === BASE_COMPARE_ID) {
      return activeScenarioId != null ? BASE_COMPARE_ID : null;
    }
    if (compareScenarioId === activeScenarioId) return null;
    return scenarios.some((s) => s.id === compareScenarioId)
      ? compareScenarioId
      : null;
  }, [compareScenarioId, activeScenarioId, scenarios]);
  const comparing = effectiveCompareId != null;
  /** Baseline id in backend terms (null = base model) — only meaningful
   * while `comparing`. */
  const baselineScenarioId =
    effectiveCompareId === BASE_COMPARE_ID ? null : effectiveCompareId;
  const baselineName =
    effectiveCompareId === BASE_COMPARE_ID
      ? "Base model"
      : (scenarios.find((s) => s.id === effectiveCompareId)?.name ??
        "Baseline");

  // Baseline result metadata — fetched once per (project, baseline,
  // resultGeneration) and cached so toggling between baselines doesn't
  // refetch. resultGeneration invalidates after any run completes.
  const [baselineMeta, setBaselineMeta] = useState<ResultMeta | null>(null);
  const [baselineMetaLoaded, setBaselineMetaLoaded] = useState(false);
  const baselineMetaCacheRef = useRef(new Map<string, ResultMeta | null>());
  useEffect(() => {
    if (!comparing || !projectId) {
      setBaselineMeta(null);
      setBaselineMetaLoaded(false);
      return;
    }
    const cache = baselineMetaCacheRef.current;
    const key = `${projectId}:${baselineScenarioId ?? BASE_COMPARE_ID}:${resultGeneration}`;
    if (cache.has(key)) {
      setBaselineMeta(cache.get(key) ?? null);
      setBaselineMetaLoaded(true);
      return;
    }
    let cancelled = false;
    setBaselineMetaLoaded(false);
    loadResultMeta(projectId, baselineScenarioId).then((m) => {
      // Bound the cache: old generations/projects are never read again.
      if (cache.size > 32) cache.clear();
      cache.set(key, m);
      if (!cancelled) {
        setBaselineMeta(m);
        setBaselineMetaLoaded(true);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [comparing, projectId, baselineScenarioId, resultGeneration]);

  // Baseline per-period results — refetched on every scrub, mirroring the
  // active-period fetch pattern (cancellation included). The period is
  // clamped to the baseline's own result length so a shorter baseline stays
  // comparable while scrubbing beyond its end (holds its last period).
  const [fetchedBaselinePeriodResult, setBaselinePeriodResult] =
    useState<PeriodResults | null>(null);

  // Same topology-stale gate as the active period result: baseline arrays are
  // also index-addressed against the live network, so baseline results whose
  // digest differs from the live model's are treated as absent (unknown
  // digests pass through ungated, matching the pre-digest behaviour).
  const baselineTopologyStale =
    compareTopologyDigests(baselineMeta?.networkDigest, liveNetworkDigest) ===
    "stale";
  const baselinePeriodResult = baselineTopologyStale
    ? null
    : fetchedBaselinePeriodResult;

  // Discard stale baseline data immediately when the baseline changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `projectId` and `effectiveCompareId` are intentional triggers to discard stale baseline data on switch.
  useEffect(() => {
    setBaselinePeriodResult(null);
  }, [projectId, effectiveCompareId]);
  useEffect(() => {
    if (!comparing || !projectId || !baselineMeta) {
      setBaselinePeriodResult(null);
      return;
    }
    let cancelled = false;
    const period = Math.max(
      0,
      Math.min(currentHour, baselineMeta.times.length - 1),
    );
    getPeriodResults(projectId, period, baselineScenarioId)
      .then((r) => {
        if (!cancelled) setBaselinePeriodResult(r);
      })
      // Decode failures reject (already logged); keep the previous baseline
      // period visible rather than crashing the effect.
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [comparing, projectId, baselineScenarioId, baselineMeta, currentHour]);

  // Delta arrays (active − baseline) — identity-stable; null while either
  // side is missing or when the element counts don't match the network
  // (topology drift → comparison unavailable). Quality deltas are dropped
  // when the two runs used different quality modes (chemical vs age vs
  // trace) — same-length arrays would otherwise subtract mg/L from hours.
  const qualityComparable =
    resultMeta?.qualityMode != null &&
    resultMeta.qualityMode === baselineMeta?.qualityMode;
  const compareDeltas = useMemo(() => {
    if (!comparing || !currentPeriodResult || !baselinePeriodResult) {
      return null;
    }
    return computeDeltas(
      currentPeriodResult,
      baselinePeriodResult,
      nodeCount,
      linkCount,
      qualityComparable,
    );
  }, [
    comparing,
    currentPeriodResult,
    baselinePeriodResult,
    nodeCount,
    linkCount,
    qualityComparable,
  ]);

  // Small dismissible notice when comparison can't run; reset on baseline switch.
  const [compareNoticeDismissed, setCompareNoticeDismissed] = useState(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `effectiveCompareId` is the intentional reset trigger for the dismissal.
  useEffect(() => {
    setCompareNoticeDismissed(false);
  }, [effectiveCompareId]);
  const compareNotice = !comparing
    ? null
    : baselineMetaLoaded && !baselineMeta
      ? "Baseline has no results"
      : baselineTopologyStale
        ? // Digest gate above nulled the baseline arrays — explain why.
          "Baseline results predate the current network topology; re-run to compare"
        : currentPeriodResult && baselinePeriodResult && !compareDeltas
          ? "Baseline network differs; comparison unavailable"
          : null;

  // Legend inputs: max |Δ| for the active variables (SI; Legend converts to
  // display units) plus the baseline caption. Null while not comparing or
  // while deltas are unavailable — the Legend then renders normally.
  const legendCompare = compareDeltas
    ? {
        baselineName,
        nodeMaxAbs:
          compareDeltas.maxAbs[
            nodeVar === "pressure"
              ? "nodePressure"
              : nodeVar === "head"
                ? "nodeHead"
                : nodeVar === "demand"
                  ? "nodeDemand"
                  : "nodeQuality"
          ],
        linkMaxAbs:
          linkVar === "status"
            ? null
            : compareDeltas.maxAbs[
                linkVar === "flow"
                  ? "linkFlow"
                  : linkVar === "velocity"
                    ? "linkVelocity"
                    : linkVar === "headloss"
                      ? "linkHeadloss"
                      : "linkQuality"
              ],
      }
    : null;

  // Baseline picker options: Off / Base model (while a scenario is active) /
  // every scenario except the active one.
  const compareOptions = useMemo(() => {
    const opts: { value: string | null; label: string }[] = [
      { value: null, label: "Off" },
    ];
    if (activeScenarioId != null) {
      opts.push({ value: BASE_COMPARE_ID, label: "Base model" });
    }
    for (const s of scenarios) {
      if (s.id !== activeScenarioId) opts.push({ value: s.id, label: s.name });
    }
    return opts;
  }, [scenarios, activeScenarioId]);

  return {
    effectiveCompareId,
    comparing,
    baselineName,
    compareDeltas,
    compareNotice,
    compareNoticeDismissed,
    setCompareNoticeDismissed,
    legendCompare,
    compareOptions,
  };
}
