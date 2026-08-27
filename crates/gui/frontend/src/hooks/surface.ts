/**
 * Surface results access: the 2D overland sidecar (`results.2d.out`)
 * served by the backend's surface provider.
 *
 * A mesh run's surface arrives in three pieces sized to how the canvas
 * consumes them: a JSON meta (counts, reporting clock, the engine's
 * surface-variable catalog with sampled SI ranges), a one-time binary
 * geometry payload, and one binary payload of cell values per timeline
 * step. The decoders here are the payload layouts' other half — the
 * byte offsets mirror `uds_surface.rs` and are pinned by tests on both
 * sides.
 */

import { useEffect, useState } from "react";
import { tryInvoke } from "./ipc";
import type { GenericVariable } from "./results";

/** Version this decoder serves (geometry payload header). */
export const SURFACE_GEOMETRY_VERSION = 1;
/** Version this decoder serves (period payload header). */
export const SURFACE_PERIOD_VERSION = 1;

/** What the backend reports about a target's surface results. */
export interface SurfaceMeta {
  nVertices: number;
  nCells: number;
  periods: number;
  reportStepS: number;
  firstReportTS: number;
  /** The engine's surface catalog with this run's SI ranges, in
   * presentation order — the period payload's column order. */
  variables: GenericVariable[];
}

/** What the model says about its own surface, known from import. */
export interface MeshInfo {
  nVertices: number;
  nCells: number;
  /** The engine's mesh-property catalog with this mesh's own ranges:
   * what can be shown about the surface with no run behind it. */
  properties: GenericVariable[];
}

/** The mesh a viewer renders without the model: SI metres, model CRS. */
export interface SurfaceGeometry {
  nVertices: number;
  nCells: number;
  /** x, y, z per vertex (length `3 * nVertices`). */
  positions: Float64Array;
  /** Vertex indices, three per cell (length `3 * nCells`). */
  triangles: Uint32Array;
}

/** One instant's cell values, columnar in catalog order (SI). */
export interface SurfacePeriod {
  /** Run time (s). */
  t: number;
  depth: Float32Array;
  elevation: Float32Array;
  speed: Float32Array;
}

function surfaceDecodeError(detail: string): Error {
  return new Error(`surface payload decode failed: ${detail}`);
}

/** Decode the geometry payload (layout: `uds_surface.rs`). */
export function decodeSurfaceGeometry(buf: ArrayBuffer): SurfaceGeometry {
  const dv = new DataView(buf);
  if (buf.byteLength < 12) {
    throw surfaceDecodeError(
      `geometry header truncated (${buf.byteLength} bytes)`,
    );
  }
  const version = dv.getUint32(0, true);
  if (version !== SURFACE_GEOMETRY_VERSION) {
    throw surfaceDecodeError(
      `geometry version ${version}, this decoder serves ${SURFACE_GEOMETRY_VERSION}`,
    );
  }
  const nVertices = dv.getUint32(4, true);
  const nCells = dv.getUint32(8, true);
  const expected = 12 + 24 * nVertices + 12 * nCells;
  if (buf.byteLength !== expected) {
    throw surfaceDecodeError(
      `geometry payload is ${buf.byteLength} bytes, expected ${expected}`,
    );
  }
  // The header is 12 bytes, so the f64 block is 4-byte aligned but not
  // 8-byte aligned: copy through aligned buffers rather than viewing.
  const positions = new Float64Array(3 * nVertices);
  for (let i = 0; i < positions.length; i++) {
    positions[i] = dv.getFloat64(12 + 8 * i, true);
  }
  const triangles = new Uint32Array(buf.slice(12 + 24 * nVertices, expected));
  return { nVertices, nCells, positions, triangles };
}

/** Decode one instant's payload (layout: `uds_surface.rs`). */
export function decodeSurfacePeriod(buf: ArrayBuffer): SurfacePeriod {
  const dv = new DataView(buf);
  if (buf.byteLength < 16) {
    throw surfaceDecodeError(
      `period header truncated (${buf.byteLength} bytes)`,
    );
  }
  const version = dv.getUint32(0, true);
  if (version !== SURFACE_PERIOD_VERSION) {
    throw surfaceDecodeError(
      `period version ${version}, this decoder serves ${SURFACE_PERIOD_VERSION}`,
    );
  }
  const nCells = dv.getUint32(4, true);
  const expected = 16 + 12 * nCells;
  if (buf.byteLength !== expected) {
    throw surfaceDecodeError(
      `period payload is ${buf.byteLength} bytes, expected ${expected}`,
    );
  }
  const t = dv.getFloat64(8, true);
  const column = (k: number) =>
    new Float32Array(buf.slice(16 + 4 * k * nCells, 16 + 4 * (k + 1) * nCells));
  return { t, depth: column(0), elevation: column(1), speed: column(2) };
}

/** The column a catalog variable id selects, in the payload's order. */
export function surfaceColumn(
  period: SurfacePeriod,
  variableId: string,
): Float32Array | null {
  switch (variableId) {
    case "depth":
      return period.depth;
    case "elevation":
      return period.elevation;
    case "speed":
      return period.speed;
    default:
      return null;
  }
}

/**
 * Whether a target's model carries a 2D surface, from the model itself —
 * so it is answerable from import, before and without any run. `null`
 * for a model with no mesh, and outside Tauri.
 *
 * Addressed by target, not by "whatever is loaded". The canvas asks the
 * moment the active project changes, which is before that project's
 * network has loaded, so an ambient answer described the project being
 * left rather than the one being opened.
 */
export async function getMeshInfo(
  projectId: string,
  scenarioId?: string | null,
): Promise<MeshInfo | null> {
  return await tryInvoke<MeshInfo | null>("load_mesh_info", {
    projectId,
    scenarioId: scenarioId ?? null,
  });
}

/**
 * A target's mesh, for surfaces that only need to know whether there is
 * one and how big it is. Re-asked when a network loads.
 */
export function useMeshInfo(
  projectId: string | null,
  scenarioId: string | null,
  networkLoaded: boolean,
): MeshInfo | null {
  const [info, setInfo] = useState<MeshInfo | null>(null);
  useEffect(() => {
    if (!networkLoaded || !projectId) {
      setInfo(null);
      return;
    }
    let cancelled = false;
    getMeshInfo(projectId, scenarioId)
      .then((m) => {
        if (!cancelled) setInfo(m);
      })
      .catch(() => {
        if (!cancelled) setInfo(null);
      });
    return () => {
      cancelled = true;
    };
  }, [networkLoaded, projectId, scenarioId]);
  return info;
}

/**
 * A target's mesh geometry. This is the mesh the canvas draws:
 * it is the one the user has open, present before any run, and a run's
 * sidecar carries a copy of it rather than a mesh of its own.
 */
export async function getMeshGeometry(
  projectId: string,
  scenarioId?: string | null,
): Promise<SurfaceGeometry | null> {
  const buf = await tryInvoke<ArrayBuffer>("load_mesh_geometry", {
    projectId,
    scenarioId: scenarioId ?? null,
  });
  if (buf === null) return null;
  const geometry = decodeSurfaceGeometry(
    requireArrayBuffer(buf, "load_mesh_geometry"),
  );
  // Empty counts are the "no mesh" answer in the shared layout, not a
  // mesh of no cells: nothing to draw either way, but the caller asks
  // about presence, so say absent.
  return geometry.nCells > 0 ? geometry : null;
}

/**
 * Surface meta for a target, `null` when it has none — the normal state
 * for every non-mesh run (and outside Tauri), not an error.
 */
export async function getSurfaceMeta(
  projectId: string,
  scenarioId?: string | null,
): Promise<SurfaceMeta | null> {
  return await tryInvoke<SurfaceMeta | null>("load_surface_meta", {
    projectId,
    scenarioId: scenarioId ?? null,
  });
}

/**
 * The transport check every binary payload gets before decoding: a
 * backend command that stops returning `tauri::ipc::Response` arrives as
 * a JSON number array, and a silent `null` there cost a whole feature —
 * the surface fetched, decoded to nothing, and nobody was told. Loud,
 * like `getPeriodResults`.
 */
function requireArrayBuffer(buf: unknown, command: string): ArrayBuffer {
  if (buf instanceof ArrayBuffer) return buf;
  const err = surfaceDecodeError(
    `${command} returned unexpected payload type ${typeof buf} (expected ArrayBuffer)`,
  );
  console.error("[surface]", err);
  throw err;
}

/** The mesh geometry. Ask only after `getSurfaceMeta` returned one. */
export async function getSurfaceGeometry(
  projectId: string,
  scenarioId?: string | null,
): Promise<SurfaceGeometry | null> {
  const buf = await tryInvoke<ArrayBuffer>("load_surface_geometry", {
    projectId,
    scenarioId: scenarioId ?? null,
  });
  if (buf === null) return null;
  try {
    return decodeSurfaceGeometry(
      requireArrayBuffer(buf, "load_surface_geometry"),
    );
  } catch (err) {
    console.error("[surface] geometry decode failed:", err);
    throw err;
  }
}

/** One instant's surface values, by period index. */
export async function getSurfacePeriod(
  projectId: string,
  period: number,
  scenarioId?: string | null,
): Promise<SurfacePeriod | null> {
  const buf = await tryInvoke<ArrayBuffer>("load_surface_period", {
    projectId,
    period,
    scenarioId: scenarioId ?? null,
  });
  if (buf === null) return null;
  try {
    return decodeSurfacePeriod(requireArrayBuffer(buf, "load_surface_period"));
  } catch (err) {
    console.error("[surface] period decode failed:", err);
    throw err;
  }
}
