import type { Link, Node } from "../hooks";

/**
 * Compute a schematic (topological) layout for a network graph.
 *
 * Ported directly from crates/gui/frontend/src/schematicLayout.ts.
 *
 * Uses BFS from source nodes (reservoirs, tanks) to arrange nodes in
 * depth-based layers with equidistant spacing. Nodes at the same BFS
 * depth are placed vertically; layers advance horizontally.
 *
 * Returns a Map from node id → [x, y] in an arbitrary Cartesian coordinate
 * space suitable for OrthographicView.
 */
export function computeSchematicLayout(
  nodes: Node[],
  links: Link[],
): Map<string, [number, number]> {
  const SPACING_X = 120; // horizontal distance between depth layers
  const SPACING_Y = 80; // vertical distance between siblings

  // Build adjacency list (undirected — flow direction not known at layout time)
  const adj = new Map<string, Set<string>>();
  for (const n of nodes) adj.set(n.id, new Set());
  for (const l of links) {
    adj.get(l.fromId)?.add(l.toId);
    adj.get(l.toId)?.add(l.fromId);
  }

  // Identify source nodes (reservoirs, tanks) as BFS roots
  const sources = nodes.filter(
    (n) => n.type === "reservoir" || n.type === "tank",
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
    const d = depth.get(cur)!;
    for (const neighbor of adj.get(cur) ?? []) {
      if (!depth.has(neighbor)) {
        depth.set(neighbor, d + 1);
        queue.push(neighbor);
      }
    }
  }

  // Handle disconnected components
  for (const n of nodes) {
    if (!depth.has(n.id)) {
      depth.set(n.id, 0);
      queue.push(n.id);
      let i = queue.length - 1;
      while (i < queue.length) {
        const cur = queue[i++];
        const d = depth.get(cur)!;
        for (const neighbor of adj.get(cur) ?? []) {
          if (!depth.has(neighbor)) {
            depth.set(neighbor, d + 1);
            queue.push(neighbor);
          }
        }
      }
    }
  }

  // Group nodes by depth layer
  const layers = new Map<number, string[]>();
  for (const [id, d] of depth) {
    if (!layers.has(d)) layers.set(d, []);
    layers.get(d)?.push(id);
  }

  // Assign positions: x by depth, y centered within each layer
  const positions = new Map<string, [number, number]>();
  for (const [d, ids] of layers) {
    const x = d * SPACING_X;
    const totalHeight = (ids.length - 1) * SPACING_Y;
    const startY = -totalHeight / 2;
    for (let i = 0; i < ids.length; i++) {
      positions.set(ids[i], [x, startY + i * SPACING_Y]);
    }
  }

  return positions;
}
