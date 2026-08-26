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
  cellValuesAtVertices,
  groundAtVertices,
  type SurfaceEdgeData,
  type SurfacePolygonData,
  surfaceBarycentric,
  surfaceCellColors,
  surfaceCornerColors,
  surfaceEdgeData,
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
  /** Each cell's own colour, repeated per vertex — what the blend mixes
   * toward at a cell's middle. Equal to `colors` for a plain fill, where
   * the mix then has nothing to do. */
  cellColors: Uint8Array;
  /** The barycentric basis the blend weight is computed from. Static for
   * a mesh, so it survives every timeline step untouched. */
  bary: Float32Array;
  /** The field at the mesh's corners, present only while the surface is
   * blended. It is what the picture interpolates near the boundaries,
   * so it is also part of what the pointer reads. */
  vertexValues: Float32Array | null;
  /** The cells' own values, which the blend keeps at their centres.
   * `null` for a field held at the vertices (the ground), where plain
   * linear interpolation is already exact. */
  centreValues: Float32Array | null;
  /** The cells' projected corners, for reading a point. */
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
  // One triangle per cell, whether the surface is blended or not: the
  // blend is a weight the shader applies within a cell, not a finer
  // geometry. So this survives a timeline step, a variable change and
  // the blend toggle alike — only the colours move.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reprojToken re-runs this once lazily fetched proj4 defs register
  const projected = useMemo(() => {
    if (!geometry || !enabled) return null;
    const project = planProjector(sourceCrs);
    if (!project) return null;
    const data: SurfacePolygonData = surfacePolygonData(geometry, project);
    // Identity of these coordinates, from the coordinates themselves.
    // Derived rather than counted because counting meant writing to a
    // ref while rendering, which React may do twice, and which made the
    // key depend on how often this ran rather than on what it produced.
    // The extent moves whenever the projection does, including when a
    // proj4 definition lands late for a code that already had a name.
    return {
      key: `${sourceCrs}:${data.bounds?.join(",") ?? "empty"}`,
      data,
      corners: data.attributes.getPolygon.value,
      bary: surfaceBarycentric(geometry.nCells),
      edges: surfaceEdgeData(geometry, project),
    };
  }, [geometry, sourceCrs, enabled, reprojToken]);

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
  /** Blend a cell's colour into its corners' rather than painting it
   * flat. The weight is applied per pixel by `BlendedSurfaceLayer`;
   * what changes here is only which colours it is given. */
  blend = false,
): {
  variable: GenericVariable | null;
  values: Float32Array | null;
  vertexValues: Float32Array | null;
  centreValues: Float32Array | null;
  colors: Uint8Array;
  cellColors: Uint8Array;
} {
  const flat = (
    variable: GenericVariable,
    values: Float32Array,
    depth: Float32Array | null,
  ) => {
    // The plain fill hands a cell's colour to its corners too, so the
    // blend has nothing to mix and the cell reads flat.
    const colors = surfaceCellColors(values, depth, variable);
    return {
      variable,
      values,
      vertexValues: null,
      centreValues: null,
      colors,
      cellColors: colors,
    };
  };

  const footprintColors = surfaceFootprintColors(geometry.nCells);
  const footprint = {
    variable: null,
    values: null,
    vertexValues: null,
    centreValues: null,
    colors: footprintColors,
    cellColors: footprintColors,
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
    if (!blend) return flat(variable, column, periodData.depth);
    // Blended: the corners take the neighbour mean and the cell keeps
    // its own colour at its middle, so a peak stays a peak.
    const vertexValues = cellValuesAtVertices(geometry, column);
    const wetCorners = cellValuesAtVertices(geometry, periodData.depth);
    return {
      variable,
      values: column,
      vertexValues,
      centreValues: column,
      colors: surfaceCornerColors(geometry, vertexValues, wetCorners, variable),
      cellColors: surfaceCellColors(column, periodData.depth, variable),
    };
  }

  // A result variable with no instant behind it yet (the first frame
  // after a scenario switch) must not be drawn from the ground and
  // labelled as water: the label would name one thing and the map show
  // another. Fall to the ground under its own name instead.
  const property = properties.find((v) => v.id === variable.id);
  const shown = property ?? properties[0];
  if (!shown) return footprint;

  // No water mask: the ground under a dry cell is still ground.
  const values = surfaceGroundValues(geometry);
  if (!blend) return flat(shown, values, null);
  // The ground needs no averaging: the mesh holds it at the vertices.
  const vertexValues = groundAtVertices(geometry);
  return {
    variable: shown,
    values,
    vertexValues,
    // Held at the vertices, so plain linear interpolation is exact and
    // a reading needs no centre term.
    centreValues: null,
    colors: surfaceCornerColors(geometry, vertexValues, null, shown),
    cellColors: surfaceCellColors(values, null, shown),
  };
}
