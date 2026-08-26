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

import { useEffect, useMemo, useRef, useState } from "react";
import { planProjector } from "../../../canvas/coords";
import {
  cellValuesAtVertices,
  groundAtVertices,
  type SurfaceEdgeData,
  type SurfacePolygonData,
  surfaceBlendedColors,
  surfaceBlendedPolygonData,
  surfaceEdgeData,
  surfaceFillColors,
  surfaceFootprintColors,
  surfaceGroundValues,
  surfacePolygonData,
} from "../../../canvas/surfaceMesh";
import { type GenericVariable, selectedVariable } from "../../../hooks/results";
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
  /**
   * Identity of the projection these coordinates were built with.
   *
   * The canvas keys its surface layers on it, so re-projecting the mesh
   * (a changed coordinate system, or a proj4 definition arriving late)
   * yields *new* layers rather than an update to existing ones. deck.gl
   * tesselates a binary polygon layer once and caches it; a layer that
   * kept its identity through a wholesale change of coordinates drew
   * the cached geometry at the old place, which read as a mesh whose
   * outlines were right and whose fill had vanished.
   */
  key: string;
  /** The mesh itself, for readings that need its topology (the value
   * under a pointer, interpolated from the cell's own vertices). */
  geometry: SurfaceGeometry;
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
  /** The field at the mesh's corners, present only while the surface is
   * blended. It is what the picture interpolates near the boundaries,
   * so it is also part of what the pointer reads. */
  vertexValues: Float32Array | null;
  /** The cells' own values, which the blended picture keeps at their
   * centres. `null` for a field held at the vertices (the ground),
   * where plain linear interpolation is already exact. */
  centreValues: Float32Array | null;
  /** The parent cells' projected corners, for reading a point. */
  corners: Float64Array | null;
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
  smooth,
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
  /** Draw the field continuous rather than one flat colour per cell. */
  smooth?: boolean;
  /** Identity of the loaded network: a new one is a new mesh question. */
  networkToken: unknown;
}): {
  surface: CanvasSurface | null;
  surfaceMeta: SurfaceMeta | null;
  meshInfo: MeshInfo | null;
  /** The surface's variables in offer order — the legend's list, and the
   * canvas's, so the two cannot name different things. */
  surfaceVariables: GenericVariable[];
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
  // Counted rather than derived from the CRS: a projection also changes
  // when a proj4 definition lands for a code that already had a name,
  // which no string built from the inputs would show.
  const projectionEpoch = useRef(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: reprojToken re-runs this once lazily fetched proj4 defs register
  const projected = useMemo(() => {
    if (!geometry || !enabled) return null;
    const project = planProjector(sourceCrs);
    if (!project) return null;
    projectionEpoch.current += 1;
    // The blended picture is drawn from three sub-triangles per cell, so
    // its geometry differs from the plain fill's and is built only while
    // it is on show.
    const blended = smooth
      ? surfaceBlendedPolygonData(geometry, project)
      : null;
    return {
      key: `${sourceCrs}:${smooth ? "blend" : "flat"}:${projectionEpoch.current}`,
      data: blended ?? surfacePolygonData(geometry, project),
      corners: blended
        ? blended.corners
        : surfacePolygonData(geometry, project).attributes.getPolygon.value,
      edges: surfaceEdgeData(geometry, project),
    };
  }, [geometry, sourceCrs, enabled, reprojToken, smooth]);

  const shown = useMemo(
    () =>
      geometry
        ? shownSurface(
            geometry,
            meshInfo?.properties ?? [],
            meta,
            periodData,
            variableId,
            smooth,
          )
        : null,
    [geometry, meshInfo, meta, periodData, variableId, smooth],
  );

  const surface = useMemo(
    () =>
      projected && shown && geometry
        ? { geometry, ...projected, ...shown }
        : null,
    [geometry, projected, shown],
  );

  const surfaceVariables = useMemo(
    () =>
      geometry
        ? surfaceVariableList(geometry, meshInfo?.properties ?? [], meta)
        : [],
    [geometry, meshInfo, meta],
  );

  return { surface, surfaceMeta: meta, meshInfo, surfaceVariables };
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
/**
 * The surface's variables in the order they are offered, which is also
 * the order that decides the default.
 *
 * A run's variables come first when they can be shown, so a simulated
 * mesh opens on its results like every other class; the mesh's own
 * properties follow, and are all there is before a run. One list, built
 * once and given to both the legend and the canvas, so the name on the
 * legend is always the picture on the map.
 */
export function surfaceVariableList(
  geometry: SurfaceGeometry,
  properties: GenericVariable[],
  meta: SurfaceMeta | null,
): GenericVariable[] {
  const usable = meta != null && meta.nCells === geometry.nCells;
  return [...(usable ? meta.variables : []), ...properties];
}

export function shownSurface(
  geometry: SurfaceGeometry,
  /** The mesh's own properties (the ground), always available. */
  properties: GenericVariable[],
  meta: SurfaceMeta | null,
  periodData: SurfacePeriod | null,
  variableId?: string,
  /** Draw the field as a continuous surface rather than a mosaic. */
  smooth = false,
): {
  variable: GenericVariable | null;
  values: Float32Array | null;
  vertexValues: Float32Array | null;
  centreValues: Float32Array | null;
  colors: Uint8Array;
} {
  const footprint = {
    variable: null,
    values: null,
    vertexValues: null,
    centreValues: null,
    colors: surfaceFootprintColors(geometry.nCells),
  };

  // Resolved over the same list, by the same rule, as the legend that
  // names it — see `surfaceVariableList` and `selectedVariable`.
  const variable = selectedVariable(
    surfaceVariableList(geometry, properties, meta),
    variableId,
  );
  if (!variable) return footprint;

  // A result is a variable the instant has a column for; anything else
  // is a property of the mesh, and the ground is the one there is.
  const column = periodData ? surfaceColumn(periodData, variable.id) : null;
  if (column && periodData) {
    if (!smooth) {
      return {
        variable,
        values: column,
        vertexValues: null,
        centreValues: null,
        colors: surfaceFillColors(column, periodData.depth, variable),
      };
    }
    // Blended: the corners take the neighbour mean and the cell keeps
    // its own value at its centre, so a peak stays a peak.
    const vertexValues = cellValuesAtVertices(geometry, column);
    const wetCorners = cellValuesAtVertices(geometry, periodData.depth);
    return {
      variable,
      values: column,
      vertexValues,
      centreValues: column,
      colors: surfaceBlendedColors(
        geometry,
        vertexValues,
        column,
        wetCorners,
        periodData.depth,
        variable,
      ),
    };
  }

  // A result variable with no instant behind it yet (the first frame
  // after a scenario switch) must not be drawn from the ground and
  // labelled as water: the label would name one thing and the map show
  // another. Fall to the ground under its own name instead.
  const property = properties.find((v) => v.id === variable.id);
  const shown = property ?? properties[0];
  if (!shown) return footprint;

  const values = surfaceGroundValues(geometry);
  if (smooth) {
    // The ground needs no averaging: the mesh holds it at the vertices,
    // and the flat fill was this renderer throwing that away. The same
    // construction serves it exactly, since a cell's centre value is
    // already the mean of its corners.
    const vertexValues = groundAtVertices(geometry);
    return {
      variable: shown,
      values,
      vertexValues,
      // Held at the vertices, so plain linear interpolation is exact and
      // a reading needs no centre term.
      centreValues: null,
      colors: surfaceBlendedColors(
        geometry,
        vertexValues,
        values,
        null,
        null,
        shown,
      ),
    };
  }
  // No water mask: the ground under a dry cell is still ground.
  return {
    variable: shown,
    values,
    vertexValues: null,
    centreValues: null,
    colors: surfaceFillColors(values, null, shown),
  };
}
