import {
  type JunctionRow,
  type Link,
  type LinkType,
  type Node,
  type PatchItem,
  PRESSURE_THRESHOLD,
  type ReservoirRow,
  type TankRow,
} from "../../../hooks";

export type ElementKind =
  | "junction"
  | "pipe"
  | "pump"
  | "tank"
  | "reservoir"
  | "valve";

export interface PendingAdd {
  kind: ElementKind;
  tempId: string;
}

export interface PendingDelete {
  kind: ElementKind;
  id: string;
}

export interface DraftEntry {
  kind: string;
  id: string;
  field: string;
  value: number | string;
}

type NodeKind = "junction" | "tank" | "reservoir";

function isNodeKind(kind: string): kind is NodeKind {
  return kind === "junction" || kind === "tank" || kind === "reservoir";
}

/**
 * The minimal link shape the preview's rename/delete cascade detection needs.
 * Full `PipeRow`/`PumpRow`/`ValveRow` objects satisfy it structurally, but
 * callers can also pass cheap `{ id, from, to }` projections (see
 * {@link linkRefRowsFromLinks}) instead of materialising full row models.
 */
export interface LinkRefRow {
  id: string;
  from: string;
  to: string;
}

/**
 * Pure projection of the live link list to {@link LinkRefRow}s for one link
 * type. Lets DraftContext feed `buildPreviewPatches` straight from network
 * data instead of holding duplicate full row-model copies alive.
 */
export function linkRefRowsFromLinks(
  links: Link[],
  type: LinkType,
): LinkRefRow[] {
  return links
    .filter((l) => l.type === type)
    .map((l) => ({ id: l.id, from: l.fromId, to: l.toId }));
}

/*
 * Pure equivalents of the `useJunctionRows` / `useTankRows` /
 * `useReservoirRows` hooks in `hooks/editors.ts`, for callers that need row
 * models outside of render — DraftContext.saveAll derives them lazily from
 * the network snapshot at save time instead of keeping a second ~92k-row
 * copy mounted for the provider's whole lifetime. Field derivations must be
 * kept in sync with the hooks so save/validation behaviour is identical.
 */

/** Pure equivalent of `useJunctionRows` (hooks/editors.ts). */
export function junctionRowsFromNodes(nodes: Node[]): JunctionRow[] {
  return nodes
    .filter((n) => n.type === "junction")
    .map((n) => ({
      id: n.id,
      elevation: Math.round((n.elevation ?? 0) * 100) / 100,
      baseDemand: Math.round((n.baseDemand ?? 0) * 100) / 100,
      demand: n.demand ?? 0,
      pressure: n.pressure !== null ? Math.round(n.pressure * 10) / 10 : null,
      x: Math.round(n.x * 100) / 100,
      y: Math.round(n.y * 100) / 100,
      belowThreshold: n.pressure !== null && n.pressure < PRESSURE_THRESHOLD,
    }));
}

/** Pure equivalent of `useTankRows` (hooks/editors.ts). */
export function tankRowsFromNodes(nodes: Node[]): TankRow[] {
  return nodes
    .filter((n) => n.type === "tank")
    .map((n) => ({
      id: n.id,
      elevation: Math.round((n.elevation ?? 0) * 100) / 100,
      minLevel: Math.round((n.tankMinLevel ?? 0) * 100) / 100,
      maxLevel: Math.round((n.tankMaxLevel ?? 0) * 100) / 100,
      initialLevel: Math.round((n.tankInitialLevel ?? 0) * 100) / 100,
      diameter:
        n.tankDiameter != null ? Math.round(n.tankDiameter * 100) / 100 : null,
      volumeCurve: n.tankVolumeCurve ?? null,
      x: Math.round(n.x * 100) / 100,
      y: Math.round(n.y * 100) / 100,
    }));
}

/** Pure equivalent of `useReservoirRows` (hooks/editors.ts). */
export function reservoirRowsFromNodes(nodes: Node[]): ReservoirRow[] {
  return nodes
    .filter((n) => n.type === "reservoir")
    .map((n) => ({
      id: n.id,
      head: Math.round((n.elevation ?? 0) * 100) / 100,
      pattern: n.headPattern ?? null,
      x: Math.round(n.x * 100) / 100,
      y: Math.round(n.y * 100) / 100,
    }));
}

/**
 * Every element ID in the network (nodes and links of all six kinds) — the
 * pool `saveStagedElements` uses for new-ID uniqueness checks. Equivalent to
 * unioning the six per-kind row-model ID sets, since node types are exactly
 * junction/tank/reservoir and link types exactly pipe/pump/valve.
 */
export function collectAllElementIds(
  nodes: Node[],
  links: Link[],
): Set<string> {
  const ids = new Set<string>();
  for (const n of nodes) ids.add(n.id);
  for (const l of links) ids.add(l.id);
  return ids;
}

export function buildPreviewPatches(args: {
  draftEntries: DraftEntry[];
  pendingAdds: PendingAdd[];
  pendingDeletes: PendingDelete[];
  pendingDeleteKeys: Set<string>;
  pipeRowsAll: LinkRefRow[];
  pumpRowsAll: LinkRefRow[];
  valveRowsAll: LinkRefRow[];
  tempIdPrefix: string;
}): PatchItem[] {
  const {
    draftEntries,
    pendingAdds,
    pendingDeletes,
    pendingDeleteKeys,
    pipeRowsAll,
    pumpRowsAll,
    valveRowsAll,
    tempIdPrefix,
  } = args;

  const staged = draftEntries.filter(
    (e) => !pendingDeleteKeys.has(`${e.kind}:${e.id}`),
  ) as PatchItem[];

  const draftValueMap = new Map<string, number | string>();
  for (const entry of draftEntries) {
    draftValueMap.set(`${entry.kind}:${entry.id}:${entry.field}`, entry.value);
  }

  const created = pendingAdds.map<PatchItem>((p) => ({
    kind: p.kind,
    id: p.tempId,
    field: "create",
    value: "(new unsaved row)",
  }));

  const deleted = pendingDeletes.map<PatchItem>((d) => ({
    kind: d.kind,
    id: d.id,
    field: "delete",
    value: "(delete element)",
  }));

  const linkRows = [
    ...pipeRowsAll.map((r) => ({
      kind: "pipe" as ElementKind,
      id: r.id,
      from: r.from,
      to: r.to,
    })),
    ...pumpRowsAll.map((r) => ({
      kind: "pump" as ElementKind,
      id: r.id,
      from: r.from,
      to: r.to,
    })),
    ...valveRowsAll.map((r) => ({
      kind: "valve" as ElementKind,
      id: r.id,
      from: r.from,
      to: r.to,
    })),
  ].filter((r) => !pendingDeleteKeys.has(`${r.kind}:${r.id}`));

  const cascade: PatchItem[] = [];
  const cascadeSeen = new Set<string>();
  const pushCascade = (item: PatchItem) => {
    const sig =
      item.field === "delete (cascade)"
        ? `${item.kind}:${item.id}:${item.field}`
        : `${item.kind}:${item.id}:${item.field}:${String(item.value)}`;
    if (cascadeSeen.has(sig)) return;
    cascadeSeen.add(sig);
    cascade.push(item);
  };

  const renamedNodes = draftEntries
    .filter(
      (e) =>
        e.field === "id" &&
        !e.id.startsWith(tempIdPrefix) &&
        isNodeKind(e.kind) &&
        !pendingDeleteKeys.has(`${e.kind}:${e.id}`),
    )
    .map((e) => ({ oldId: e.id, newId: String(e.value).trim() }))
    .filter((e) => e.newId.length > 0 && e.newId !== e.oldId);

  for (const rename of renamedNodes) {
    for (const link of linkRows) {
      const explicitFrom = draftValueMap.has(`${link.kind}:${link.id}:from`);
      const explicitTo = draftValueMap.has(`${link.kind}:${link.id}:to`);
      const effectiveFrom = String(
        draftValueMap.get(`${link.kind}:${link.id}:from`) ?? link.from,
      );
      const effectiveTo = String(
        draftValueMap.get(`${link.kind}:${link.id}:to`) ?? link.to,
      );

      if (!explicitFrom && effectiveFrom === rename.oldId) {
        pushCascade({
          kind: link.kind,
          id: link.id,
          field: "from (cascade)",
          value: rename.newId,
        });
      }
      if (!explicitTo && effectiveTo === rename.oldId) {
        pushCascade({
          kind: link.kind,
          id: link.id,
          field: "to (cascade)",
          value: rename.newId,
        });
      }
    }
  }

  for (const del of pendingDeletes) {
    if (!isNodeKind(del.kind)) continue;
    for (const link of linkRows) {
      const effectiveFrom = String(
        draftValueMap.get(`${link.kind}:${link.id}:from`) ?? link.from,
      );
      const effectiveTo = String(
        draftValueMap.get(`${link.kind}:${link.id}:to`) ?? link.to,
      );
      if (effectiveFrom === del.id || effectiveTo === del.id) {
        pushCascade({
          kind: link.kind,
          id: link.id,
          field: "delete (cascade)",
          value: `because node '${del.id}' is deleted`,
        });
      }
    }
  }

  return [...created, ...staged, ...deleted, ...cascade];
}

export function collectDirtyKinds(args: {
  draftEntries: DraftEntry[];
  pendingAdds: PendingAdd[];
  pendingDeletes: PendingDelete[];
}): Set<string> {
  const { draftEntries, pendingAdds, pendingDeletes } = args;
  const kinds = new Set<string>();
  for (const entry of draftEntries) kinds.add(entry.kind);
  for (const pending of pendingAdds) kinds.add(pending.kind);
  for (const pending of pendingDeletes) kinds.add(pending.kind);
  return kinds;
}
