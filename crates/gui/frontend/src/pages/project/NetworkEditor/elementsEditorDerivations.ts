import type { PatchItem, PipeRow, PumpRow, ValveRow } from "../../../hooks";

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

export function buildPreviewPatches(args: {
  draftEntries: DraftEntry[];
  pendingAdds: PendingAdd[];
  pendingDeletes: PendingDelete[];
  pendingDeleteKeys: Set<string>;
  pipeRowsAll: PipeRow[];
  pumpRowsAll: PumpRow[];
  valveRowsAll: ValveRow[];
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
