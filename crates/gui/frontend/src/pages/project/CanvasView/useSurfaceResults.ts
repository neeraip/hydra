/**
 * Surface results for the canvas: the 2D overland mesh and one instant's
 * cell colours, fetched and shaped for MapCanvas's surface layer.
 *
 * Owns: the per-target surface meta + geometry fetch (reloaded with the
 * same freshness token as the network results), the per-instant value
 * fetch on the shared timeline index, the plan-coordinate projection of
 * the mesh (the same forward transform the node pipeline applies), and
 * the colour build. Everything it computes is `null` for a target with
 * no surface results — every non-mesh run — so the canvas below stays
 * surface-blind unless there is a surface to draw.
 */

import { useEffect, useMemo, useState } from "react";

import { planProjector } from "../../../canvas/coords";
import {
  type SurfacePolygonData,
  surfaceFillColors,
  surfacePolygonData,
} from "../../../canvas/surfaceMesh";
import {
  getSurfaceGeometry,
  getSurfaceMeta,
  getSurfacePeriod,
  type SurfaceGeometry,
  type SurfaceMeta,
  type SurfacePeriod,
  surfaceColumn,
} from "../../../hooks/surface";

/** What MapCanvas draws: binary polygons and per-vertex colours. */
export interface CanvasSurface {
  data: SurfacePolygonData;
  colors: Uint8Array;
}

export function useSurfaceResults({
  projectId,
  scenarioId,
  resultMetaKey,
  period,
  sourceCrs,
  reprojToken,
  enabled,
  variableId,
}: {
  projectId: string | null;
  scenarioId: string | null;
  /** Freshness token shared with the network results: a new run is a new
   * key, and the surface reloads with it. `null` = no results settled. */
  resultMetaKey: string | null;
  /** Clamped timeline period index; `null` when the timeline is empty. */
  period: number | null;
  sourceCrs: string;
  /** Re-run token for lazily registered proj4 defs: changes when the node
   * reprojection lands, at which point the mesh's CRS resolves too. */
  reprojToken: unknown;
  /** False outside map mode (a schematic's positions are invented; the
   * mesh's are real) — clears the surface without dropping the fetch. */
  enabled: boolean;
  /** Selected surface variable id ("" = the catalog's first, depth). */
  variableId?: string;
}): { surface: CanvasSurface | null; surfaceMeta: SurfaceMeta | null } {
  const [meta, setMeta] = useState<SurfaceMeta | null>(null);
  const [geometry, setGeometry] = useState<SurfaceGeometry | null>(null);
  const [periodData, setPeriodData] = useState<SurfacePeriod | null>(null);

  // Meta + geometry, once per target per run. Cleared only when the
  // absence is settled, mirroring the network period fetch's latch.
  useEffect(() => {
    if (!projectId || resultMetaKey == null) {
      setMeta(null);
      setGeometry(null);
      setPeriodData(null);
      return;
    }
    let cancelled = false;
    getSurfaceMeta(projectId, scenarioId)
      .then(async (m) => {
        if (cancelled) return;
        if (!m) {
          setMeta(null);
          setGeometry(null);
          setPeriodData(null);
          return;
        }
        const g = await getSurfaceGeometry(projectId, scenarioId);
        if (cancelled) return;
        setMeta(m);
        setGeometry(g);
      })
      .catch(() => {
        if (!cancelled) {
          setMeta(null);
          setGeometry(null);
          setPeriodData(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, resultMetaKey]);

  // One instant's values, on the shared timeline index. The sidecar is
  // written at the same reporting instants as results.out, so the same
  // period index addresses both.
  useEffect(() => {
    if (!projectId || meta == null || period == null) {
      setPeriodData(null);
      return;
    }
    let cancelled = false;
    // The network timeline can carry more periods than the surface if a
    // run was interrupted mid-instant; clamp rather than surface an
    // out-of-range refusal as a toast.
    const p = Math.min(period, meta.periods - 1);
    if (p < 0) {
      setPeriodData(null);
      return;
    }
    getSurfacePeriod(projectId, p, scenarioId)
      .then((r) => {
        if (!cancelled) setPeriodData(r);
      })
      .catch(() => {
        if (!cancelled) setPeriodData(null);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, meta, period]);

  // Geometry → screen-space polygons: re-runs on CRS change or def
  // registration, never on a timeline scrub.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reprojToken re-runs this once lazily fetched proj4 defs register
  const polygons = useMemo(() => {
    if (!geometry || !enabled) return null;
    const project = planProjector(sourceCrs);
    if (!project) return null;
    return surfacePolygonData(geometry, project);
  }, [geometry, sourceCrs, enabled, reprojToken]);

  // Colours: the selected variable through the engine's ramp, dry cells
  // transparent. An id the catalog does not carry (a stale preference)
  // falls back to the first variable, the legend's own rule.
  const colors = useMemo(() => {
    if (!meta || !periodData || !polygons) return null;
    const variable =
      meta.variables.find((v) => v.id === variableId) ?? meta.variables[0];
    if (!variable) return null;
    const values = surfaceColumn(periodData, variable.id);
    if (!values) return null;
    return surfaceFillColors(values, periodData.depth, variable);
  }, [meta, periodData, polygons, variableId]);

  const surface = useMemo(
    () => (polygons && colors ? { data: polygons, colors } : null),
    [polygons, colors],
  );
  return { surface, surfaceMeta: meta };
}
