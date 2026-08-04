import type { Link, Node } from "../hooks";
import type { Region } from "../types";

/**
 * Compute a schematic (topological) layout for a network graph.
 *
 * Uses BFS from source nodes (reservoirs, tanks) to arrange nodes in
 * depth-based layers with equidistant spacing. Nodes at the same BFS
 * depth are placed vertically; layers advance horizontally.
 *
 * Returns positions (a Map from node id → [x, y] in an arbitrary Cartesian
 * space suitable for OrthographicView) together with the ids that are not
 * reachable from any source. The caller needs that set to mark the detached
 * group on the canvas — position alone cannot say *why* a cluster sits apart.
 *
 * `scale` stretches the two spacings independently — `{x: 1, y: 1}` is the
 * layout this function has always produced. Radii and link widths are layer
 * properties and are untouched, so only the layout's proportions move.
 *
 * Scaling both axes equally is not worth exposing: it is arithmetically the
 * same as zooming (see `schematicSpacing`). The ratio between them is the part
 * zoom cannot reach, and it is what turns a tall thin spike or a wide flat fan
 * into something readable.
 */
/** Minimum blank columns between the connected network and the detached group. */
const DETACHED_GAP_COLUMNS = 4;
/** …and at least this share of the network's own width, so the separation stays
 * visible on layouts that are tens of thousands of units wide. */
const DETACHED_GAP_FRACTION = 0.06;

/** How much of a layer's spacing a catchment glyph may occupy. Small
 * enough that a glyph never reaches its neighbouring column, large enough
 * to read as an area rather than a marker. */
const GLYPH_FRACTION = 0.42;

export interface SchematicLayout {
  positions: Map<string, [number, number]>;
  /** Ids not reachable from any boundary node — a genuinely separate
   * subnetwork. Empty for a fully connected network. */
  detachedIds: Set<string>;
  /** Region boundaries in schematic space, keyed by region id.
   *
   * A schematic keeps a catchment's *shape* but not its position: real
   * rings are plan geometry, and the schematic has no plan. Each ring is
   * therefore normalised to a fixed glyph size and anchored beside the
   * node it drains to — how a drainage schematic has always drawn a
   * catchment, as a symbol hung off its outlet. Regions the layout could
   * not anchor are absent. */
  regionRings: Map<string, Array<[number, number]>>;
  /** Glyph centre → outlet node position, for the leader line that says
   * which node a glyph drains to. Same keys as `regionRings` minus any
   * region whose outlet is not a node. */
  regionLeaders: Map<string, [[number, number], [number, number]]>;
}

/** Place each region's ring beside the node it drains to, at a size the
 * schematic's own spacing dictates. Shape is preserved (one scale factor
 * for both axes); absolute position and size are not, because in a
 * schematic they mean nothing. */
function placeRegionGlyphs(
  regions: Region[],
  positions: Map<string, [number, number]>,
  spacingX: number,
  spacingY: number,
): Pick<SchematicLayout, "regionRings" | "regionLeaders"> {
  const regionRings = new Map<string, Array<[number, number]>>();
  const regionLeaders = new Map<string, [[number, number], [number, number]]>();
  if (regions.length === 0) return { regionRings, regionLeaders };

  const boxW = spacingX * GLYPH_FRACTION;
  const boxH = spacingY * GLYPH_FRACTION;

  // Several catchments may drain to one node, so they fan out along the
  // column rather than stacking invisibly on top of each other.
  const perOutlet = new Map<string, number>();

  for (const r of regions) {
    if (r.ring.length < 3) continue;
    const outletId = r.outletId;
    if (!outletId) continue;
    const anchor = positions.get(outletId);
    if (!anchor) continue;

    let minX = Number.POSITIVE_INFINITY;
    let minY = Number.POSITIVE_INFINITY;
    let maxX = Number.NEGATIVE_INFINITY;
    let maxY = Number.NEGATIVE_INFINITY;
    for (const [x, y] of r.ring) {
      if (!Number.isFinite(x) || !Number.isFinite(y)) continue;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
    const w = maxX - minX;
    const h = maxY - minY;
    if (!Number.isFinite(w) || !Number.isFinite(h) || (w <= 0 && h <= 0)) {
      continue;
    }
    // One factor for both axes: a catchment squeezed to fill a box is no
    // longer that catchment's shape.
    const scale = Math.min(
      w > 0 ? boxW / w : boxH / h,
      h > 0 ? boxH / h : boxW / w,
    );
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;

    const slot = perOutlet.get(outletId) ?? 0;
    perOutlet.set(outletId, slot + 1);
    // Up and to the left of the outlet — upstream of it in a layout whose
    // depth grows rightward — then fanned upward per additional catchment.
    const gx = anchor[0] - spacingX * 0.5;
    const gy = anchor[1] - spacingY * (0.75 + slot * GLYPH_FRACTION * 1.3);

    // Plan coordinates grow northward; the orthographic canvas grows
    // downward. Flipping y here keeps the glyph the right way up.
    regionRings.set(
      r.id,
      r.ring.map(
        ([x, y]: [number, number]) =>
          [gx + (x - cx) * scale, gy - (y - cy) * scale] as [number, number],
      ),
    );
    regionLeaders.set(r.id, [[gx, gy], anchor]);
  }
  return { regionRings, regionLeaders };
}

/** A connection that is not a link: a conduit coupled to a node by
 * something other than shared endpoints (a dual-drainage street inlet). */
export interface LayoutCoupling {
  link: string;
  node: string;
}

/** Boundary nodes the layout treats as roots: reservoirs/tanks for water
 * distribution, outfalls for drainage. */
function boundaryNodes(nodes: Node[]): Node[] {
  const found = nodes.filter(
    (n) => n.type === "reservoir" || n.type === "tank" || n.type === "outfall",
  );
  if (found.length === 0 && nodes.length > 0) return [nodes[0]];
  return found;
}

/** Ids not reachable from any boundary node — a genuinely separate
 * subnetwork, by whatever adjacency the caller built. */
function detachedFrom(
  nodes: Node[],
  adj: Map<string, Set<string>>,
): Set<string> {
  const seen = new Set<string>();
  const queue = boundaryNodes(nodes).map((n) => n.id);
  for (const id of queue) seen.add(id);
  let head = 0;
  while (head < queue.length) {
    const cur = queue[head++];
    for (const neighbor of adj.get(cur) ?? []) {
      if (!seen.has(neighbor)) {
        seen.add(neighbor);
        queue.push(neighbor);
      }
    }
  }
  return new Set(nodes.filter((n) => !seen.has(n.id)).map((n) => n.id));
}

export function computeSchematicLayout(
  nodes: Node[],
  links: Link[],
  scale: { x: number; y: number } = { x: 1, y: 1 },
  couplings: LayoutCoupling[] = [],
  /** Keep the model's own plan coordinates instead of laying nodes out by
   * depth. For a model on a local grid this view *is* the plan — the
   * coordinates are real, they simply are not georeferenced, so there is
   * no map to put them on but every reason to keep their true shape. */
  realCoords = false,
  /** Catchment boundaries to place alongside the nodes. */
  regions: Region[] = [],
): SchematicLayout {
  // Every position below is linear in these two constants, so scaling them is
  // equivalent to scaling the output per axis — no re-running the BFS.
  const SPACING_X = 120 * scale.x; // horizontal distance between depth layers
  const SPACING_Y = 80 * scale.y; // vertical distance between siblings

  // Build adjacency list (undirected — flow direction not known at layout time)
  const adj = new Map<string, Set<string>>();
  for (const n of nodes) adj.set(n.id, new Set());
  for (const l of links) {
    adj.get(l.fromId)?.add(l.toId);
    adj.get(l.toId)?.add(l.fromId);
  }
  // Inlet couplings join a conduit to a node without sharing an endpoint,
  // so connectivity derived from links alone misses them — a street
  // network draining into a sewer through inlets would read as detached.
  // The whole segment drains to the capture node, so both of its endpoints
  // are adjacent to it.
  if (couplings.length > 0) {
    const linkById = new Map(links.map((l) => [l.id, l]));
    for (const c of couplings) {
      const l = linkById.get(c.link);
      if (!l || !adj.has(c.node)) continue;
      for (const end of [l.fromId, l.toId]) {
        if (!adj.has(end)) continue;
        adj.get(end)?.add(c.node);
        adj.get(c.node)?.add(end);
      }
    }
  }

  // Identify boundary nodes as BFS roots: reservoirs/tanks for water
  // distribution, outfalls for drainage (where flow converges rather than
  // diverges — the layout only needs consistent depths, not direction).
  const sources = nodes.filter(
    (n) => n.type === "reservoir" || n.type === "tank" || n.type === "outfall",
  );
  if (sources.length === 0 && nodes.length > 0) sources.push(nodes[0]);

  if (realCoords) {
    const positions = new Map<string, [number, number]>();
    // Coordinates arrive in the canvas's own space already (a local grid
    // is flipped once at reprojection), so they are used as given.
    for (const n of nodes) {
      positions.set(n.id, [n.x * scale.x, n.y * scale.y]);
    }
    // Real coordinates make this the plan view, so rings are drawn where
    // the model puts them — scaled with the nodes so the two stay aligned
    // once the aspect slider moves.
    const regionRings = new Map<string, Array<[number, number]>>();
    for (const r of regions) {
      if (r.ring.length < 3) continue;
      regionRings.set(
        r.id,
        r.ring.map(
          ([x, y]: [number, number]) =>
            [x * scale.x, y * scale.y] as [number, number],
        ),
      );
    }
    return {
      positions,
      detachedIds: detachedFrom(nodes, adj),
      regionRings,
      regionLeaders: new Map(),
    };
  }

  // BFS to assign each node a depth
  const depth = new Map<string, number>();
  const queue: string[] = [];
  for (const s of sources) {
    if (!depth.has(s.id)) {
      depth.set(s.id, 0);
      queue.push(s.id);
    }
  }

  let head = 0;
  while (head < queue.length) {
    const cur = queue[head++];
    const d = depth.get(cur);
    if (d == null) continue;
    for (const neighbor of adj.get(cur) ?? []) {
      if (!depth.has(neighbor)) {
        depth.set(neighbor, d + 1);
        queue.push(neighbor);
      }
    }
  }

  // Lay the connected network out first: x by BFS depth, y centred within each
  // layer.
  const place = (
    depths: Map<string, number>,
  ): Map<string, [number, number]> => {
    const layers = new Map<number, string[]>();
    for (const [id, d] of depths) {
      const layer = layers.get(d);
      if (layer) layer.push(id);
      else layers.set(d, [id]);
    }
    const out = new Map<string, [number, number]>();
    for (const [d, ids] of layers) {
      const x = d * SPACING_X;
      const startY = (-(ids.length - 1) * SPACING_Y) / 2;
      for (let i = 0; i < ids.length; i++) {
        out.set(ids[i], [x, startY + i * SPACING_Y]);
      }
    }
    return out;
  };

  const positions = place(depth);

  // Anything the source BFS never reached: a node whose last link was deleted,
  // one added before it was connected, or a whole sub-network with no source.
  //
  // These become a separate group to the *right* of the connected network,
  // clear of it by a gap that scales with the network's own width. Depth 0 put
  // them in the leftmost column among the reservoirs, so losing one pipe looked
  // like the node had teleported across the diagram; a couple of trailing
  // columns was no better, because that gap is nothing beside a network
  // thousands of units wide and still read as part of the fan.
  const detachedDepth = new Map<string, number>();
  for (const n of nodes) {
    if (depth.has(n.id) || detachedDepth.has(n.id)) continue;
    // Every detached component starts at depth 0, so components share columns
    // and stack vertically rather than marching right once per component — but
    // each still keeps its own internal BFS shape, because a detached chain is
    // still a chain.
    detachedDepth.set(n.id, 0);
    const queue = [n.id];
    let head = 0;
    while (head < queue.length) {
      const cur = queue[head++];
      const d = detachedDepth.get(cur) ?? 0;
      for (const neighbor of adj.get(cur) ?? []) {
        if (!depth.has(neighbor) && !detachedDepth.has(neighbor)) {
          detachedDepth.set(neighbor, d + 1);
          queue.push(neighbor);
        }
      }
    }
  }

  if (detachedDepth.size > 0) {
    const detached = place(detachedDepth);

    let connectedMaxX = Number.NEGATIVE_INFINITY;
    let connectedMinX = Number.POSITIVE_INFINITY;
    let connectedMinY = Number.POSITIVE_INFINITY;
    for (const [x, y] of positions.values()) {
      if (x > connectedMaxX) connectedMaxX = x;
      if (x < connectedMinX) connectedMinX = x;
      if (y < connectedMinY) connectedMinY = y;
    }
    // Everything detached (no connected nodes at all): anchor at the origin.
    if (!Number.isFinite(connectedMaxX)) {
      connectedMaxX = 0;
      connectedMinX = 0;
      connectedMinY = 0;
    }

    let detachedMinX = Number.POSITIVE_INFINITY;
    let detachedMinY = Number.POSITIVE_INFINITY;
    for (const [x, y] of detached.values()) {
      if (x < detachedMinX) detachedMinX = x;
      if (y < detachedMinY) detachedMinY = y;
    }

    // The gap scales with the network: a fixed few columns is invisible beside a
    // layout tens of thousands of units wide, which is exactly the size where
    // finding a stray node matters most.
    const connectedWidth = connectedMaxX - connectedMinX;
    const gap = Math.max(
      DETACHED_GAP_COLUMNS * SPACING_X,
      connectedWidth * DETACHED_GAP_FRACTION,
    );
    const dx = connectedMaxX + gap - detachedMinX;
    // Top-aligned with the connected network, so the group reads as a column
    // beside it rather than floating at an arbitrary height.
    const dy = connectedMinY - detachedMinY;
    for (const [id, [x, y]] of detached) {
      positions.set(id, [x + dx, y + dy]);
    }
  }

  return {
    positions,
    detachedIds: new Set(detachedDepth.keys()),
    // Placed last, once every node — connected and detached — has landed,
    // so a catchment draining to a stray node follows it there.
    ...placeRegionGlyphs(regions, positions, SPACING_X, SPACING_Y),
  };
}
