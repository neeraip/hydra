import type { ScenarioDto } from "../../../hooks";

export interface FlatScenario extends ScenarioDto {
  depth: number;
}

/** One node of the scenario forest built by {@link buildScenarioTree}. */
export interface ScenarioTreeNode extends ScenarioDto {
  children: ScenarioTreeNode[];
}

/**
 * Build the scenario forest from the flat `list_scenarios` rows. Children
 * keep their list order. Defensive against bad data:
 *
 * - orphans (parent id not present in the list) become roots;
 * - self-parented rows become roots;
 * - cycle members (unreachable from any root) are broken by promoting the
 *   first-listed member of each cycle to a root (detaching it from its
 *   parent), so every row appears exactly once and walks always terminate.
 */
export function buildScenarioTree(dtos: ScenarioDto[]): ScenarioTreeNode[] {
  const nodes = new Map<string, ScenarioTreeNode>();
  for (const d of dtos) nodes.set(d.id, { ...d, children: [] });
  const roots: ScenarioTreeNode[] = [];
  for (const d of dtos) {
    const node = nodes.get(d.id);
    if (!node) continue;
    const parent = d.parentScenarioId ? nodes.get(d.parentScenarioId) : null;
    if (parent && parent !== node) parent.children.push(node);
    else roots.push(node);
  }
  // Promote cycle members: anything not reachable from a root is part of a
  // parent cycle. Detach the first unreached row from its parent and treat
  // it as a root, then re-mark (its cycle-mates become its descendants).
  const reachable = new Set<string>();
  const mark = (n: ScenarioTreeNode) => {
    if (reachable.has(n.id)) return;
    reachable.add(n.id);
    for (const c of n.children) mark(c);
  };
  for (const r of roots) mark(r);
  for (const d of dtos) {
    if (reachable.has(d.id)) continue;
    const node = nodes.get(d.id);
    if (!node) continue;
    const parent = d.parentScenarioId ? nodes.get(d.parentScenarioId) : null;
    if (parent) {
      parent.children = parent.children.filter((c) => c.id !== node.id);
    }
    roots.push(node);
    mark(node);
  }
  return roots;
}

/**
 * DFS flatten preserving parent→child adjacency: each scenario is followed
 * immediately by its descendants (depth tracks indentation). Mirrors the
 * ordering used by OverviewView's ScenarioList. Orphans and cycle members
 * surface at depth 0 (see {@link buildScenarioTree}).
 */
export function flattenScenarios(dtos: ScenarioDto[]): FlatScenario[] {
  const result: FlatScenario[] = [];
  const walk = (node: ScenarioTreeNode, depth: number) => {
    const { children, ...dto } = node;
    result.push({ ...dto, depth });
    for (const child of children) walk(child, depth + 1);
  };
  for (const root of buildScenarioTree(dtos)) walk(root, 0);
  return result;
}

/**
 * Root-first ancestor path of `activeScenarioId` (inclusive): Base's child
 * first, the active scenario last. Returns `[]` for `null` or an id not in
 * the list (the caller falls back to the Base lineage). A missing parent id
 * ends the walk (orphan treated as root); a parent cycle breaks the walk at
 * the first revisit instead of looping.
 */
export function activeLineage(
  dtos: ScenarioDto[],
  activeScenarioId: string | null,
): ScenarioDto[] {
  if (!activeScenarioId) return [];
  const byId = new Map(dtos.map((d) => [d.id, d]));
  let cur = byId.get(activeScenarioId);
  if (!cur) return [];
  const path: ScenarioDto[] = [];
  const seen = new Set<string>();
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    path.push(cur);
    cur = cur.parentScenarioId ? byId.get(cur.parentScenarioId) : undefined;
  }
  return path.reverse();
}

/**
 * Direct children of `parentId` (`null` = scenarios branched straight off
 * the base model), in list order. Self-parented rows are never their own
 * child.
 */
export function scenarioChildren(
  dtos: ScenarioDto[],
  parentId: string | null,
): ScenarioDto[] {
  return dtos.filter(
    (d) => (d.parentScenarioId ?? null) === parentId && d.id !== parentId,
  );
}

/** What the project toolbar's strip shows immediately after a variant chip. */
export type VariantTail =
  | { kind: "none" }
  | { kind: "child"; child: ScenarioDto }
  | { kind: "dropdown"; count: number };

/**
 * Summarise one variant's subtree for the toolbar strip.
 *
 * The strip stays one level deep per variant, so a subtree collapses to at most
 * one trailing entry: nothing when there is nothing below, the child itself
 * when the whole subtree *is* that one child, and a picker otherwise. A lone
 * child that has children of its own still gets the picker — inlining it would
 * imply the branch ended there.
 */
export function variantTail(
  dtos: ScenarioDto[],
  variantId: string,
): VariantTail {
  const children = scenarioChildren(dtos, variantId);
  if (children.length === 0) return { kind: "none" };
  if (
    children.length === 1 &&
    scenarioChildren(dtos, children[0].id).length === 0
  ) {
    return { kind: "child", child: children[0] };
  }
  return { kind: "dropdown", count: children.length };
}

/**
 * DFS-flatten the subtrees rooted at `rootIds` (each root at depth 0), in
 * the order the ids are given. Unknown ids are skipped. Built on
 * {@link buildScenarioTree}, so orphan/cycle defenses apply.
 */
export function flattenSubtrees(
  dtos: ScenarioDto[],
  rootIds: readonly string[],
): FlatScenario[] {
  const byId = new Map<string, ScenarioTreeNode>();
  const collect = (n: ScenarioTreeNode) => {
    byId.set(n.id, n);
    for (const c of n.children) collect(c);
  };
  for (const root of buildScenarioTree(dtos)) collect(root);
  const result: FlatScenario[] = [];
  const walk = (node: ScenarioTreeNode, depth: number) => {
    const { children, ...dto } = node;
    result.push({ ...dto, depth });
    for (const child of children) walk(child, depth + 1);
  };
  for (const id of rootIds) {
    const node = byId.get(id);
    if (node) walk(node, 0);
  }
  return result;
}

/**
 * Breadcrumb label for a scenario: its lineage names root-first joined with
 * " ▸ " (e.g. `Drought ▸ Stage 2 ▸ Night demand`). Empty string for ids not
 * in the list.
 */
export function lineageLabel(dtos: ScenarioDto[], id: string): string {
  return activeLineage(dtos, id)
    .map((s) => s.name)
    .join(" ▸ ");
}

export const STATE_LABEL: Record<string, string> = {
  "not-run": "Not run",
  running: "Running…",
  simulated: "Simulated",
  failed: "Failed",
};

export const STATE_COLOR: Record<string, string> = {
  "not-run": "var(--text-tertiary)",
  running: "#d9aa57",
  simulated: "#7bbf95",
  failed: "#d97b7b",
};

export const rowButtonStyle: React.CSSProperties = {
  fontSize: "var(--text-sm)",
  fontWeight: 500,
  padding: "3px 9px",
  border: "1px solid var(--border-hover)",
  borderRadius: 5,
  background: "transparent",
  color: "var(--text-secondary)",
  cursor: "pointer",
  fontFamily: "var(--font-ui)",
  whiteSpace: "nowrap",
};

export const iconButtonStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 26,
  height: 26,
  border: "none",
  borderRadius: 5,
  background: "transparent",
  color: "var(--text-tertiary)",
  cursor: "pointer",
  padding: 0,
  transition: "background 0.1s, color 0.1s",
};

/**
 * Scenarios branched directly from `scenarioId`.
 *
 * Deliberately direct children only, not all descendants. Deleting a scenario
 * removes one directory; `buildScenarioTree` then promotes rows whose parent
 * id no longer resolves to roots. Only the immediate children are orphaned
 * that way — a grandchild still resolves its own parent and keeps its place —
 * so these are exactly the rows whose lineage the deletion changes.
 */
export function directChildren(
  dtos: ScenarioDto[],
  scenarioId: string,
): ScenarioDto[] {
  return dtos.filter((d) => d.parentScenarioId === scenarioId);
}

/**
 * Every scenario descended from `scenarioId`, at any depth.
 *
 * Distinct from {@link directChildren}, and both are needed to describe a
 * deletion honestly: the whole subtree *survives* it — each scenario is a
 * complete copy of its parent's model, not a delta — while only the direct
 * children visibly move, since a grandchild still resolves its own parent.
 *
 * Walks with a visited set rather than recursing on parent links. The raw
 * rows can contain parent cycles (see {@link buildScenarioTree}, which
 * defends against the same thing), and a naive walk would not terminate.
 */
export function descendants(
  dtos: ScenarioDto[],
  scenarioId: string,
): ScenarioDto[] {
  const byParent = new Map<string, ScenarioDto[]>();
  for (const d of dtos) {
    if (!d.parentScenarioId) continue;
    const siblings = byParent.get(d.parentScenarioId);
    if (siblings) siblings.push(d);
    else byParent.set(d.parentScenarioId, [d]);
  }
  const found: ScenarioDto[] = [];
  const seen = new Set<string>([scenarioId]);
  const queue = [scenarioId];
  while (queue.length > 0) {
    // biome-ignore lint/style/noNonNullAssertion: guarded by queue.length.
    const id = queue.shift()!;
    for (const child of byParent.get(id) ?? []) {
      if (seen.has(child.id)) continue;
      seen.add(child.id);
      found.push(child);
      queue.push(child.id);
    }
  }
  return found;
}
