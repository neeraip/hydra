/**
 * The 2D overland surface for the canvas: the model's mesh, and the
 * values a run painted onto it.
 *
 * Two sources, deliberately separated. The **mesh** comes from the model
 * — it is present from import, so a mesh model shows its surface before
 * it has ever been run, and the canvas draws the mesh the user actually
 * has open. The **values** come from the run's `.2d.out` sidecar, which
 * carries its own copy of the mesh only so a viewer could render without
 * a model; here that copy serves one purpose, as the check that the
 * run's values belong to the mesh on screen.
 *
 * Everything is `null` for a model with no mesh — every water model and
 * most drainage ones — so the canvas below stays surface-blind unless
 * there is a surface.
 */

import { useEffect, useMemo, useState } from "react";
import { planProjector } from "../../../canvas/coords";
import {
  type SurfaceEdgeData,
  type SurfacePolygonData,
  surfaceEdgeData,
  surfaceFillColors,
  surfaceFootprintColors,
  surfacePolygonData,
} from "../../../canvas/surfaceMesh";
import type { GenericVariable } from "../../../hooks/results";
import {
  getMeshGeometry,
  getMeshInfo,
  getSurfaceMeta,
  getSurfacePeriod,
  type MeshInfo,
  type SurfaceGeometry,
  type SurfaceMeta,
  type SurfacePeriod,
  surfaceColumn,
} from "../../../hooks/surface";

/** What MapCanvas draws, and what the hover chip reads. */
export interface CanvasSurface {
  data: SurfacePolygonData;
  /** The mesh's own structure, drawn where its cells are big enough on
   * screen to be told apart (the canvas decides, per camera). */
  edges: SurfaceEdgeData;
  colors: Uint8Array;
  /** The variable the colours carry, or `null` when the surface is drawn
   * as a footprint — a mesh with no run behind it yet. */
  variable: GenericVariable | null;
  /** That variable's per-cell SI values, `null` alongside `variable`. */
  values: Float32Array | null;
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
  networkToken,
}: {
  projectId: string | null;
  scenarioId: string | null;
  /** Freshness token shared with the network results: a new run is a new
   * key, and the surface values reload with it. `null` = no results. */
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
  /** Identity of the loaded network: a new one is a new mesh question. */
  networkToken: unknown;
}): {
  surface: CanvasSurface | null;
  surfaceMeta: SurfaceMeta | null;
  meshInfo: MeshInfo | null;
} {
  const [meshInfo, setMeshInfo] = useState<MeshInfo | null>(null);
  const [geometry, setGeometry] = useState<SurfaceGeometry | null>(null);
  const [meta, setMeta] = useState<SurfaceMeta | null>(null);
  const [periodData, setPeriodData] = useState<SurfacePeriod | null>(null);

  // Does this model carry a mesh? Cheap, and asked of the model, so the
  // answer holds before any run.
  // biome-ignore lint/correctness/useExhaustiveDependencies: networkToken is the re-ask signal, see above
  useEffect(() => {
    if (!projectId) {
      setMeshInfo(null);
      return;
    }
    let cancelled = false;
    getMeshInfo()
      .then((m) => {
        if (!cancelled) setMeshInfo(m);
      })
      .catch(() => {
        if (!cancelled) setMeshInfo(null);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, networkToken]);

  // The mesh itself, once per model. Keyed on its counts rather than on
  // the network's identity: the geometry can run to megabytes, and every
  // ordinary edit leaves the mesh alone (the app has no mesh editor).
  const meshKey = meshInfo
    ? `${projectId}:${scenarioId ?? "base"}:${meshInfo.nVertices}:${meshInfo.nCells}`
    : null;
  useEffect(() => {
    if (meshKey == null) {
      setGeometry(null);
      return;
    }
    let cancelled = false;
    getMeshGeometry()
      .then((g) => {
        if (!cancelled) setGeometry(g);
      })
      .catch(() => {
        if (!cancelled) setGeometry(null);
      });
    return () => {
      cancelled = true;
    };
  }, [meshKey]);

  // The run's surface values, if this target has been run. Asked only of
  // a mesh model: nothing else writes a sidecar.
  useEffect(() => {
    if (!projectId || meshInfo == null || resultMetaKey == null) {
      setMeta(null);
      setPeriodData(null);
      return;
    }
    let cancelled = false;
    getSurfaceMeta(projectId, scenarioId)
      .then((m) => {
        if (cancelled) return;
        setMeta(m);
        if (!m) setPeriodData(null);
      })
      .catch(() => {
        if (!cancelled) {
          setMeta(null);
          setPeriodData(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, meshInfo, resultMetaKey]);

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

  // Geometry → screen space: the cells to fill and the edges to draw
  // over them. Re-runs on a CRS change or a def registration, never on a
  // timeline scrub — this is the mesh, and the mesh does not move.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reprojToken re-runs this once lazily fetched proj4 defs register
  const projected = useMemo(() => {
    if (!geometry || !enabled) return null;
    const project = planProjector(sourceCrs);
    if (!project) return null;
    return {
      data: surfacePolygonData(geometry, project),
      edges: surfaceEdgeData(geometry, project),
    };
  }, [geometry, sourceCrs, enabled, reprojToken]);

  const shown = useMemo(
    () =>
      geometry ? shownSurface(geometry, meta, periodData, variableId) : null,
    [geometry, meta, periodData, variableId],
  );

  const surface = useMemo(
    () => (projected && shown ? { ...projected, ...shown } : null),
    [projected, shown],
  );
  return { surface, surfaceMeta: meta, meshInfo };
}

/**
 * What the mesh is painted with: a run's values, or the footprint that
 * says "there is a surface here" before any run.
 *
 * Values are used only where they belong to the mesh on screen. A run
 * whose cell count differs is a run of a *different* mesh, and painting
 * cell `i` of this one with cell `i` of that one is a confident wrong
 * answer — the shape of defect this codebase keeps finding, one index
 * answering two questions. Exported and pure so that rule is a thing a
 * test can hold, rather than a branch buried in an effect.
 */
export function shownSurface(
  geometry: SurfaceGeometry,
  meta: SurfaceMeta | null,
  periodData: SurfacePeriod | null,
  variableId?: string,
): {
  variable: GenericVariable | null;
  values: Float32Array | null;
  colors: Uint8Array;
} {
  const footprint = {
    variable: null,
    values: null,
    colors: surfaceFootprintColors(geometry.nCells),
  };
  if (meta == null || periodData == null) return footprint;
  if (meta.nCells !== geometry.nCells) return footprint;
  const variable =
    meta.variables.find((v) => v.id === variableId) ?? meta.variables[0];
  if (!variable) return footprint;
  const values = surfaceColumn(periodData, variable.id);
  if (!values) return footprint;
  return {
    variable,
    values,
    colors: surfaceFillColors(values, periodData.depth, variable),
  };
}
