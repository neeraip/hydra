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
import { useNetworkVersion } from "./NetworkVersionContext";
import {
  fetchNetworkSnapshot,
  listenNetworkChangedPayload,
  type PatchedElement,
} from "./network";

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

/**
 * The backend omits always-null optional DTO fields from its JSON instead of
 * serialising explicit `null`s (a ~40% snapshot payload cut at 46k nodes).
 * `Node.pressure` / `Node.demand` are non-optional (`number | null`) in the
 * frontend type and some consumers compare them with strict `!== null`, so
 * fill them back in on receipt. Mutates in place — callers pass freshly
 * fetched, not-yet-shared arrays.
 */
export function normalizeNodes(nodes: Node[]): Node[] {
  for (const n of nodes) {
    if (n.pressure === undefined) n.pressure = null;
    if (n.demand === undefined) n.demand = null;
  }
  return nodes;
}

/**
 * Replace the element with the same id, returning a new array for React
 * state identity. An id not present in `items` is deliberately **dropped**,
 * never appended: creates and deletes always emit payload-less
 * `network-changed` events (a full-refetch signal), and Tauri delivers
 * events in order, so a delta referencing an unknown id can only be a stale
 * patch arriving in the window between such a structural event and its
 * refetch completing. Appending it would resurrect a just-deleted element
 * or insert a half-initialised one ("ghost elements"); dropping is safe
 * because the already-scheduled refetch supersedes it.
 */
export function patchById<T extends { id: string }>(
  items: T[],
  updated: T,
): T[] {
  const idx = items.findIndex((el) => el.id === updated.id);
  if (idx < 0) return items;
  const next = items.slice();
  next[idx] = updated;
  return next;
}

/** Apply each patch in order (unknown ids are dropped — see `patchById`).
 *  Untouched entries keep their object identity (the perf contract that lets
 *  memoised consumers skip re-render work); returns the input array unchanged
 *  when `updates` is empty or nothing matches. */
export function patchAllById<T extends { id: string }>(
  items: T[],
  updates: T[],
): T[] {
  let next = items;
  for (const u of updates) next = patchById(next, u);
  return next;
}

/**
 * Pure equivalent of the `network-changed` delta path: apply the patched
 * element DTOs from a delta payload to the current node/link arrays and
 * return the next arrays. Patched nodes are normalised in place (see
 * `normalizeNodes`). Patches for ids not present are dropped (see
 * `patchById`). Arrays with no matching patches are returned by reference,
 * and untouched entries keep their identity.
 */
export function applyElementDeltas(
  nodes: Node[],
  links: Link[],
  elements: PatchedElement[],
): { nodes: Node[]; links: Link[] } {
  const patchedNodes = elements.flatMap((el) => (el.node ? [el.node] : []));
  const patchedLinks = elements.flatMap((el) => (el.link ? [el.link] : []));
  return {
    nodes:
      patchedNodes.length > 0
        ? patchAllById(nodes, normalizeNodes(patchedNodes))
        : nodes,
    links: patchAllById(links, patchedLinks),
  };
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
  const { version, bumpNetwork } = useNetworkVersion();
  const [nodes, setNodes] = useState<Node[]>([]);
  const [links, setLinks] = useState<Link[]>([]);
  const [loading, setLoading] = useState(false);
  // Set only by `primeNetworkData`: the snapshot was just loaded via
  // `load_project_network`, so the version bump that follows a prime must
  // not trigger a redundant `get_network_snapshot` fetch.
  const skipNextFetchRef = useRef(false);
  // Set when a `network-changed` event without a delta payload arrives (a
  // structural mutation) or when an in-flight fetch is cancelled before it
  // could deliver: the next version-triggered fetch must run even if a prime
  // in the same batch requested a skip.
  const fullRefetchNeededRef = useRef(false);
  // True while a `get_network_snapshot` fetch is outstanding — lets the
  // delta listener detect that a snapshot response (possibly encoded before
  // the delta's mutation) could land after the delta and silently revert it.
  const fetchInFlightRef = useRef(false);

  const primeNetworkData = useCallback((snapshot: NetworkSnapshotDto) => {
    skipNextFetchRef.current = true;
    fullRefetchNeededRef.current = false;
    setNodes(normalizeNodes(snapshot.nodes));
    setLinks(snapshot.links);
    setLoading(false);
  }, []);

  // Apply single-element deltas from `network-changed` events in place.
  // Element-scoped edits (patch_element / patch_elements /
  // patch_node_position) carry the updated DTOs, so a 92k-element snapshot
  // refetch per edit is unnecessary; events without a payload (create /
  // delete / structural changes) still trigger the full refetch below via
  // the version bump from NetworkVersionContext (which bumps only for such
  // structural events — deltas are fully self-applied here).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    listenNetworkChangedPayload((payload) => {
      // Inline equivalent of `isStructuralNetworkChange` (kept inline so TS
      // narrows `payload` to non-null below).
      if (!payload || payload.elements.length === 0) {
        fullRefetchNeededRef.current = true;
        return;
      }
      if (fetchInFlightRef.current) {
        // A snapshot fetch is outstanding. Its response may have been
        // encoded before this delta's mutation, so letting it land after we
        // apply the delta locally would silently revert the edit. Force a
        // fresh fetch instead (the bump cancels the in-flight one, and the
        // cleanup below marks it as needing a refetch).
        fullRefetchNeededRef.current = true;
        bumpNetwork();
      }
      const patchedNodes = payload.elements.flatMap((el) =>
        el.node ? [el.node] : [],
      );
      const patchedLinks = payload.elements.flatMap((el) =>
        el.link ? [el.link] : [],
      );
      if (patchedNodes.length > 0) {
        normalizeNodes(patchedNodes);
        setNodes((prev) => patchAllById(prev, patchedNodes));
      }
      if (patchedLinks.length > 0) {
        setLinks((prev) => patchAllById(prev, patchedLinks));
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [bumpNetwork]);

  useEffect(() => {
    if (skipNextFetchRef.current && !fullRefetchNeededRef.current) {
      skipNextFetchRef.current = false;
      setLoading(false);
      return;
    }
    skipNextFetchRef.current = false;
    fullRefetchNeededRef.current = false;

    let cancelled = false;
    let settled = false;
    setLoading(true);
    fetchInFlightRef.current = true;
    const fetchStartedAt = performance.now();

    // Binary snapshot fetch + decode (see `decodeNetworkSnapshot`). The
    // decoder already emits explicit nulls, so `normalizeNodes` finds nothing
    // left to fill in — kept as the single normalisation seam shared with the
    // JSON delta path.
    fetchNetworkSnapshot()
      .then((snapshot) => {
        if (cancelled) return;
        const nextNodes = normalizeNodes(snapshot?.nodes ?? []);
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
      .catch((err: unknown) => {
        // A decode failure is a frontend/backend layout mismatch — surface it
        // loudly and keep the previous data instead of silently rendering an
        // empty network. (Command failures are already reported to the app
        // shell via `tryInvoke`'s onIpcError inside fetchNetworkSnapshot.)
        console.error("[network] get_network_snapshot decode failed:", err);
      })
      .finally(() => {
        settled = true;
        if (!cancelled) {
          fetchInFlightRef.current = false;
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
      if (!settled) {
        // The fetch was cancelled mid-flight, so its snapshot is discarded.
        // Unless the cancelling run was primed with fresh data
        // (primeNetworkData → skipNextFetchRef), whatever change triggered
        // this fetch never reached state — make the next run refetch instead
        // of skipping. (Before this guard, a delta landing during an
        // in-flight fetch discarded the fetch AND skipped the next run,
        // losing the structural change the fetch was carrying.)
        fetchInFlightRef.current = false;
        if (!skipNextFetchRef.current) fullRefetchNeededRef.current = true;
      }
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
