import { useEffect, useMemo, useRef, useState } from "react";
import {
  ensureEpsgDef,
  LOCAL_CRS,
  normalizeEpsgCode,
  registerCustomCrsDefinitions,
  reprojectLinkVerticesCached,
  reprojectNodesCached,
  reprojectRegions,
} from "../../../canvas/coords";
import { useCanvasStatus } from "../../../canvas/status-context";
import { type Link, listCrsCatalogPage, type Node } from "../../../hooks";
import type { Region } from "../../../types";

/** Coverage of real map coordinates across the network's nodes. */
export type CoordStatus = "complete" | "partial" | "empty";

/**
 * CRS reprojection pipeline for the canvas' geographic (map) mode.
 *
 * EPANET [COORDINATES] carry no CRS tag. We default to WGS84 (pass-through);
 * when the project's persisted source CRS differs, raw node x/y and link
 * polyline vertices are reprojected to WGS84 before they reach MapCanvas.
 * Schematic mode uses the BFS layout so it is never affected by CRS.
 *
 * Owns: the sourceCrs mirror of the project row, lazy proj4 def resolution
 * from the CRS catalog (with a `crsResolving` flag so callers can suppress
 * error UI mid-lookup), reprojection error surfacing, coordinate-coverage
 * classification (pushed to the shared canvas-status context for the TopBar
 * indicator), and per-element identity caches so a single-element edit costs
 * one proj4 call instead of a full-network re-transform.
 */
export function useCrsReprojection({
  projectSourceCrs,
  baseNodes,
  baseLinks,
  baseRegions,
}: {
  /** The project row's persisted CRS (EPSG code); absent → WGS84. */
  projectSourceCrs: string | null | undefined;
  baseNodes: Node[];
  baseLinks: Link[];
  baseRegions: Region[];
}): {
  sourceCrs: string;
  crsError: string | null;
  /** True while a catalog proj4 def is being fetched for the source CRS. */
  crsResolving: boolean;
  coordStatus: CoordStatus;
  coordMissingCount: number;
  /** Raw positional nodes (alias of baseNodes) for CRS sniffing/sampling. */
  rawPositionNodes: Node[];
  /** Base nodes with reprojected x/y merged (identity-stable when WGS84). */
  posNodes: Node[];
  /** Links with reprojected polyline vertices (identity-stable when WGS84). */
  canvasLinks: Link[];
  /** Regions with reprojected boundary rings (identity-stable when WGS84). */
  canvasRegions: Region[];
} {
  const { setCoordStatus } = useCanvasStatus();

  // Initialise from the persisted project value so the reprojection survives
  // session restarts. Falls back to WGS84 for draft projects (project is null).
  const [sourceCrs, setSourceCrs] = useState<string>(
    projectSourceCrs ?? "EPSG:4326",
  );
  const [crsError, setCrsError] = useState<string | null>(null);

  // Keep sourceCrs in sync if the project row changes (e.g. loaded from disk
  // while the canvas is already open).
  useEffect(() => {
    setSourceCrs(projectSourceCrs ?? "EPSG:4326");
  }, [projectSourceCrs]);

  // Ensure a proj4 definition exists for a projected source CRS before the
  // reprojection memo runs. On a cold start only baseline (4326/3857) and
  // auto-generatable (UTM/MGA) codes are known up front; a catalog EPSG like a
  // state-plane zone has no def until the CRS modal fetches it. Without this, a
  // persisted non-WGS84 CRS would fail to reproject after a restart and surface
  // a spurious "Invalid coordinate reference system" popup. Look the code up in
  // the catalog, register it, and bump a version so the memo re-runs.
  const [crsDefsVersion, setCrsDefsVersion] = useState(0);
  const [crsResolving, setCrsResolving] = useState(false);
  useEffect(() => {
    // ensureEpsgDef registers baseline/UTM/MGA defs as a side effect and
    // returns true when the code is (now) usable — nothing more to do.
    if (
      sourceCrs === "EPSG:4326" ||
      sourceCrs === LOCAL_CRS ||
      ensureEpsgDef(sourceCrs)
    ) {
      setCrsResolving(false);
      return;
    }
    let cancelled = false;
    setCrsResolving(true);
    void (async () => {
      try {
        const page = await listCrsCatalogPage({
          query: sourceCrs,
          page: 0,
          pageSize: 100,
        });
        if (cancelled) return;
        const entry = page.items.find(
          (e) => normalizeEpsgCode(e.epsg) === sourceCrs,
        );
        if (entry?.proj4?.trim()) {
          registerCustomCrsDefinitions([entry]);
          // Re-run the reprojection memo now that the def is registered.
          setCrsDefsVersion((v) => v + 1);
        }
      } finally {
        if (!cancelled) setCrsResolving(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sourceCrs]);

  // Raw positional nodes (no pressure/demand merged yet) used for CRS sniffing
  // and reprojection. Stable across timeline scrubs.
  const rawPositionNodes = baseNodes;

  // Clear any prior reprojection error when the network identity changes
  // (new load or project switch); the reprojection memo below re-derives it.
  useEffect(() => {
    if (rawPositionNodes.length === 0) return;
    setCrsError(null);
  }, [rawPositionNodes.length]);

  // Classify how many nodes have real map coordinates.
  // The Rust backend emits (0, 0) for nodes that have no [COORDINATES] entry in
  // the INP — so x === 0 && y === 0 is the sentinel for "missing".
  const coordMissingCount = useMemo(
    () => rawPositionNodes.filter((n) => n.x === 0 && n.y === 0).length,
    [rawPositionNodes],
  );
  const coordStatus = useMemo((): CoordStatus => {
    if (rawPositionNodes.length === 0) return "complete"; // nothing loaded yet
    if (coordMissingCount === 0) return "complete";
    if (coordMissingCount === rawPositionNodes.length) return "empty";
    return "partial";
  }, [rawPositionNodes, coordMissingCount]);

  // Push coord status to the shared context so the TopBar breadcrumb indicator
  // can read it without prop-drilling through ProjectPage.
  useEffect(() => {
    setCoordStatus(coordStatus, coordMissingCount, rawPositionNodes.length);
  }, [coordStatus, coordMissingCount, rawPositionNodes.length, setCoordStatus]);

  // Apply reprojection to the raw positional nodes. Result is memo-ised so
  // pressure/velocity scrubs (which don't change x/y) don't re-run proj4.
  // proj4 errors are surfaced via `crsError` (set in the effect below, not
  // here — setting state inside useMemo is a React anti-pattern).
  // Per-node reprojection cache: on a projected CRS every network mutation
  // delivers a fresh baseNodes array, but almost all coordinates are
  // unchanged — reuse both the proj4 result and the output object (identity)
  // for nodes whose source object is identical, so a single-element patch
  // costs one proj4 call instead of 46k.
  const reprojCacheRef = useRef<{
    crs: string;
    byId: Map<
      string,
      {
        src: (typeof baseNodes)[number];
        out: (typeof baseNodes)[number];
      }
    >;
  }>({ crs: "", byId: new Map() });
  // crsDefsVersion is a deliberate re-run token — the memo reads global proj4
  // defs registered by the CRS-resolution effect, a side effect biome cannot
  // see as a dependency.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-run token, see above
  const reprojection = useMemo(() => {
    // A local drawing grid is not georeferenced, so there is nothing to
    // reproject and no range to be outside of. Coordinates pass through
    // as the model states them.
    if (sourceCrs === LOCAL_CRS) {
      // The orthographic view's Y axis grows downward while model
      // coordinates grow northward, so a local grid is flipped once here —
      // for nodes, link vertices and region rings alike — and every
      // consumer downstream then works in one consistent space.
      return {
        nodes: rawPositionNodes.map((n) => ({ ...n, y: -n.y })),
        error: null as string | null,
      };
    }
    if (sourceCrs === "EPSG:4326") {
      // Even with the default CRS, check that the raw coordinates are within
      // WGS84 range. If any node is out of range the user has projected
      // coordinates and needs to set the source CRS.
      const outOfRange = rawPositionNodes.some(
        (n) =>
          !(n.x === 0 && n.y === 0) &&
          (n.x < -180 || n.x > 180 || n.y < -90 || n.y > 90),
      );
      if (outOfRange) {
        return {
          nodes: rawPositionNodes,
          error: "Coordinates are outside WGS84 range.",
        } as { nodes: typeof rawPositionNodes; error: string | null };
      }
      return { nodes: rawPositionNodes, error: null as string | null };
    }
    try {
      const cache = reprojCacheRef.current;
      if (cache.crs !== sourceCrs) {
        cache.crs = sourceCrs;
        cache.byId = new Map();
      }
      const nodes = reprojectNodesCached(
        rawPositionNodes,
        sourceCrs,
        cache.byId,
      );
      return { nodes, error: null };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { nodes: rawPositionNodes, error: msg };
    }
    // crsDefsVersion re-runs this once a lazily-fetched catalog proj4 def is
    // registered (see the CRS-resolution effect above).
  }, [sourceCrs, rawPositionNodes, crsDefsVersion]);

  // Surface reprojection errors to the toolbar without setting state during
  // render.  Runs after every reprojection result.
  useEffect(() => {
    setCrsError(reprojection.error);
  }, [reprojection.error]);

  const reprojectedPositionNodes = reprojection.nodes;

  // Build a lookup from node id → reprojected [x, y] so posNodes below can
  // merge reprojected positions without touching pressure/demand.
  const reprojectedXY = useMemo(() => {
    const m = new Map<string, { x: number; y: number }>();
    // Identity match means coordinates are already WGS84 — posNodes will
    // return baseNodes untouched, so skip building a 46k-entry Map.
    if (reprojectedPositionNodes === baseNodes) return m;
    for (const n of reprojectedPositionNodes) m.set(n.id, { x: n.x, y: n.y });
    return m;
  }, [reprojectedPositionNodes, baseNodes]);
  // Base nodes with reprojected x/y merged — deliberately independent of
  // period results so its identity is stable across timeline scrubs. The
  // canvas reads sim values from the flat arrays (periodResult prop) instead.
  const posNodes = useMemo(() => {
    // No reprojection ran (EPSG:4326 in range): reuse baseNodes as-is —
    // identity stability here keeps MapCanvas's data memos from rebuilding.
    if (reprojectedXY.size === 0) return baseNodes;
    return baseNodes.map((n) => {
      const pos = reprojectedXY.get(n.id);
      return pos ? { ...n, x: pos.x, y: pos.y } : n;
    });
  }, [baseNodes, reprojectedXY]);

  // Link polyline vertices are stored in the source CRS exactly like node
  // coords, so they go through the same proj4 transform (with the same
  // EPSG:4326 identity fast-path and a per-link identity cache). Errors are
  // already surfaced by the node reprojection above — fall back to the raw
  // links so map+schematic keep rendering.
  const linkReprojCacheRef = useRef<{
    crs: string;
    byId: Map<
      string,
      { src: (typeof baseLinks)[number]; out: (typeof baseLinks)[number] }
    >;
  }>({ crs: "", byId: new Map() });
  const canvasLinks = useMemo(() => {
    if (sourceCrs === LOCAL_CRS) {
      return baseLinks.map((l) =>
        l.vertices
          ? {
              ...l,
              vertices: l.vertices.map(([x, y]) => [x, -y] as [number, number]),
            }
          : l,
      );
    }
    if (sourceCrs === "EPSG:4326") return baseLinks;
    try {
      const cache = linkReprojCacheRef.current;
      if (cache.crs !== sourceCrs) {
        cache.crs = sourceCrs;
        cache.byId = new Map();
      }
      return reprojectLinkVerticesCached(baseLinks, sourceCrs, cache.byId);
    } catch {
      return baseLinks;
    }
  }, [sourceCrs, baseLinks]);

  // Region rings live in the source CRS exactly like link vertices; same
  // transform, same fall-back-to-raw on error (already surfaced above).
  const canvasRegions = useMemo(() => {
    if (baseRegions.length === 0) return baseRegions;
    if (sourceCrs === LOCAL_CRS) {
      return baseRegions.map((r) => ({
        ...r,
        ring: r.ring.map(([x, y]) => [x, -y] as [number, number]),
      }));
    }
    if (sourceCrs === "EPSG:4326") {
      return baseRegions;
    }
    try {
      return reprojectRegions(baseRegions, sourceCrs);
    } catch {
      return baseRegions;
    }
  }, [sourceCrs, baseRegions]);

  return {
    sourceCrs,
    crsError,
    crsResolving,
    coordStatus,
    coordMissingCount,
    rawPositionNodes,
    posNodes,
    canvasLinks,
    canvasRegions,
  };
}
