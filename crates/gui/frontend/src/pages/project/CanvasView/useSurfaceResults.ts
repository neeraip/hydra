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

import { useCallback, useEffect, useMemo, useState } from "react";
import { planProjector } from "../../../canvas/coords";
import {
  groundAtVertices,
  type SurfaceEdgeData,
  type SurfacePolygonData,
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

/**
 * State that is only an answer about the target it was fetched for.
 *
 * Every fetch here is asynchronous and the target changes the instant the
 * user picks another project, so the answers always land late. Held as a
 * plain value, the previous project's mesh stays readable through the
 * whole of a switch: the canvas drew it over the new network, and the
 * camera fit framed the union of the two, which is what "switching
 * projects does not fit the network" turned out to be. Held with its
 * target, an answer for somewhere else simply reads as no answer, in the
 * same render the target changes, with nothing to catch up.
 */
function useTargeted<T>(
  target: string | null,
): [T | null, (of: string, value: T | null) => void] {
  const [held, setHeld] = useState<Held<T>>(null);
  const set = useCallback((of: string, value: T | null) => {
    setHeld({ target: of, value });
  }, []);
  return [answerFor(held, target), set];
}

/** A held answer: a value, and the target it is an answer about. */
export type Held<T> = { target: string; value: T | null } | null;

/**
 * The held answer, if it is an answer about `target`.
 *
 * The whole of the rule, in one line, so it can be stated in a test:
 * an answer about another target is not a lesser answer, it is no
 * answer. Nothing about a project you are no longer looking at may be
 * drawn on the canvas or framed by the camera.
 */
export function answerFor<T>(held: Held<T>, target: string | null): T | null {
  return held && target != null && held.target === target ? held.value : null;
}

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
  /** Per drawn vertex, RGBA. One colour repeated across a cell's three
   * vertices paints it flat; three different ones let the rasteriser
   * interpolate, which is the smooth drawing. */
  colors: Uint8Array;
  /** The variable the colours carry, or `null` when the surface is drawn
   * as a footprint — which is not the state before a run (that is the
   * ground), but the one where no variable can be shown at all. */
  variable: GenericVariable | null;
  /** That variable's per-cell SI values, `null` alongside `variable`. */
  values: Float32Array | null;
  /** The field at the mesh's corners, present only while the surface is
   * drawn smooth. What the picture interpolates, and what the pointer
   * reads inside a cell. */
  vertexValues: Float32Array | null;
  /** The cells' projected corners, for reading a point. */
  corners: Float64Array | null;
  /** Whether the field is drawn continuous rather than one flat colour
   * per cell. Stated, not inferred from the colour array: what the
   * canvas needs to know is the reading mode, and a footprint or a
   * one-cell-per-colour fill is not one however its colours look. */
  blended: boolean;
  /** Whether the shown variable could be drawn continuous at all — a
   * field the mesh holds at its vertices. The legend offers its toggle
   * on this, so the control is absent over a run's values rather than
   * present and lying. */
  smoothable: boolean;
  /**
   * The fill layer's binary `data`, ready to hand to deck.
   *
   * Built here, and only here, because deck decides whether to
   * re-tesselate by comparing the *identity* of this object with the one
   * it last saw: a fresh literal, however identical its contents, is "a
   * new data container" and rebuilds the whole mesh's index buffers. The
   * canvas rebuilds its layer list for many reasons that have nothing to
   * do with the surface — a hovered node, a changed tool, a selection —
   * so building it there meant re-tesselating 100k triangles because the
   * pointer crossed a junction. Tied to the surface instead, it changes
   * when the surface does and not once more.
   */
  layerData: {
    length: number;
    startIndices: Uint32Array;
    attributes: Record<string, { value: ArrayBufferView; size: number }>;
  };
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
  // Everything below is held under the target it was fetched for, and
  // read only while that is still the target on screen. See `useTargeted`:
  // an answer about the project you just left is worse than no answer,
  // because the canvas draws it and the camera frames it.
  const target = projectId ? `${projectId}:${scenarioId ?? "base"}` : null;
  const [meshInfo, setMeshInfo] = useTargeted<MeshInfo>(target);
  const [geometry, setGeometry] = useTargeted<SurfaceGeometry>(target);
  const [meta, setMeta] = useTargeted<SurfaceMeta>(target);
  const [periodData, setPeriodData] = useTargeted<SurfacePeriod>(target);

  // Does this model carry a mesh? Cheap, and asked of the model, so the
  // answer holds before any run.
  // biome-ignore lint/correctness/useExhaustiveDependencies: networkToken is the re-ask signal, see above
  useEffect(() => {
    if (!projectId || !target) return;
    let cancelled = false;
    getMeshInfo(projectId, scenarioId)
      .then((m) => {
        if (!cancelled) setMeshInfo(target, m);
      })
      .catch(() => {
        if (!cancelled) setMeshInfo(target, null);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, target, setMeshInfo, networkToken]);

  // The mesh itself, once per model. Keyed on its counts rather than on
  // the network's identity: the geometry can run to megabytes, and every
  // ordinary edit leaves the mesh alone (the app has no mesh editor).
  const meshKey = meshInfo
    ? `${projectId}:${scenarioId ?? "base"}:${meshInfo.nVertices}:${meshInfo.nCells}`
    : null;
  useEffect(() => {
    if (meshKey == null || !projectId || !target) return;
    let cancelled = false;
    getMeshGeometry(projectId, scenarioId)
      .then((g) => {
        if (!cancelled) setGeometry(target, g);
      })
      .catch(() => {
        if (!cancelled) setGeometry(target, null);
      });
    return () => {
      cancelled = true;
    };
  }, [meshKey, projectId, scenarioId, target, setGeometry]);

  // The run's surface values, if this target has been run. Asked only of
  // a mesh model: nothing else writes a sidecar.
  useEffect(() => {
    if (!projectId || !target || meshInfo == null || resultMetaKey == null) {
      // Cleared, not merely left unfetched: this is the path a deleted
      // result set takes, and a held meta would go on offering the
      // variables of a run that no longer exists.
      if (target) {
        setMeta(target, null);
        setPeriodData(target, null);
      }
      return;
    }
    let cancelled = false;
    getSurfaceMeta(projectId, scenarioId)
      .then((m) => {
        if (cancelled) return;
        setMeta(target, m);
        if (!m) setPeriodData(target, null);
      })
      .catch(() => {
        if (!cancelled) {
          setMeta(target, null);
          setPeriodData(target, null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    projectId,
    scenarioId,
    target,
    setMeta,
    setPeriodData,
    meshInfo,
    resultMetaKey,
  ]);

  // One instant's values, on the shared timeline index. The sidecar is
  // written at the same reporting instants as results.out, so the same
  // period index addresses both.
  useEffect(() => {
    if (!projectId || !target || meta == null || period == null) {
      // Same reason as above: an empty timeline must not leave the last
      // instant painted on the mesh.
      if (target) setPeriodData(target, null);
      return;
    }
    let cancelled = false;
    // The network timeline can carry more periods than the surface if a
    // run was interrupted mid-instant; clamp rather than surface an
    // out-of-range refusal as a toast.
    const p = Math.min(period, meta.periods - 1);
    if (p < 0) {
      setPeriodData(target, null);
      return;
    }
    getSurfacePeriod(projectId, p, scenarioId)
      .then((r) => {
        if (!cancelled) setPeriodData(target, r);
      })
      .catch(() => {
        if (!cancelled) setPeriodData(target, null);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, target, setPeriodData, meta, period]);

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
        ? {
            geometry,
            ...projected,
            ...shown,
            layerData: {
              length: projected.data.length,
              startIndices: projected.data.startIndices,
              attributes: {
                getPolygon: projected.data.attributes.getPolygon,
                // Per drawn vertex. deck interpolates a vertex colour
                // attribute across the triangle, so the same colour
                // three times paints a cell flat and three different
                // ones draw the plane through them.
                getFillColor: { value: shown.colors, size: 4 },
              },
            },
          }
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
  /** The mesh's own properties (the ground), always available. */
  properties: GenericVariable[],
  meta: SurfaceMeta | null,
  periodData: SurfacePeriod | null,
  variableId?: string,
  /** Draw a field the mesh holds at its vertices as the continuous
   * surface it is, by colouring the vertices. Ignored for a run's
   * values, which are held per cell and have no vertex reading to
   * draw — see `smooth`. */
  smooth = false,
): {
  variable: GenericVariable | null;
  values: Float32Array | null;
  /** The field at the mesh's corners, present only while the surface is
   * drawn smooth. What the picture interpolates, and what the pointer
   * reads. */
  vertexValues: Float32Array | null;
  colors: Uint8Array;
  /** Whether the picture is continuous across cell boundaries. */
  blended: boolean;
  /** Whether the shown variable *could* be drawn continuous: a field the
   * mesh holds at its vertices. A run's values are not, so the toggle is
   * not offered over them. */
  smoothable: boolean;
} {
  const flat = (
    variable: GenericVariable,
    values: Float32Array,
    depth: Float32Array | null,
    smoothable: boolean,
  ) => ({
    variable,
    values,
    vertexValues: null,
    blended: false,
    smoothable,
    colors: surfaceCellColors(values, depth, variable),
  });

  const footprintColors = surfaceFootprintColors(geometry.nCells);
  const footprint = {
    variable: null,
    values: null,
    vertexValues: null,
    blended: false,
    smoothable: false,
    colors: footprintColors,
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
    // Always flat. A cell is the solver's unit of state and the engine
    // publishes no vertex reading, so there is nothing between cell
    // centres that is ours to draw.
    return flat(variable, column, periodData.depth, false);
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
  if (!smooth) return flat(shown, values, null, true);
  // The mesh holds the ground at its vertices, so colouring them and
  // letting the rasteriser interpolate draws the plane through three
  // known elevations. The flat drawing is the one that invents: it
  // shows all three as their mean.
  const vertexValues = groundAtVertices(geometry);
  return {
    variable: shown,
    values,
    vertexValues,
    blended: true,
    smoothable: true,
    colors: surfaceCornerColors(geometry, vertexValues, shown),
  };
}
