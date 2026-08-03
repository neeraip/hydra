import type { Link, Node } from "../hooks";

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

export interface SchematicLayout {
  positions: Map<string, [number, number]>;
  /** Ids not reachable from any reservoir or tank — the detached group. Empty
   * for a fully connected network. */
  detachedIds: Set<string>;
}

export function computeSchematicLayout(
  nodes: Node[],
  links: Link[],
  scale: { x: number; y: number } = { x: 1, y: 1 },
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

  // Identify boundary nodes as BFS roots: reservoirs/tanks for water
  // distribution, outfalls for drainage (where flow converges rather than
  // diverges — the layout only needs consistent depths, not direction).
  const sources = nodes.filter(
    (n) => n.type === "reservoir" || n.type === "tank" || n.type === "outfall",
  );
  if (sources.length === 0 && nodes.length > 0) sources.push(nodes[0]);

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

  return { positions, detachedIds: new Set(detachedDepth.keys()) };
}
