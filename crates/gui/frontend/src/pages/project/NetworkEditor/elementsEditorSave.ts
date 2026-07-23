import {
  createLink,
  createNode,
  deleteElement,
  type JunctionRow,
  type PatchItem,
  patchElements,
  type ReservoirRow,
  type TankRow,
} from "../../../hooks";
import type {
  DraftEntry,
  ElementKind,
  PendingAdd,
  PendingDelete,
} from "./elementsEditorDerivations";

export interface SaveStagedElementsArgs {
  draftEntries: DraftEntry[];
  pendingAdds: PendingAdd[];
  pendingDeletes: PendingDelete[];
  pendingDeleteKeys: Set<string>;
  junctionRowsAll: JunctionRow[];
  tankRowsAll: TankRow[];
  reservoirRowsAll: ReservoirRow[];
  allElementIds: Set<string>;
  tempIdPrefix: string;
}

export interface SaveStagedElementsResult {
  applied: number;
  failed: number;
  errors: string[];
}

/**
 * Collect all staged field edits into the `PatchItem[]` for a single bulk
 * `patch_elements` call: one IPC round trip, one INP dirty-flag set, and one
 * `network-changed` event for the whole batch, instead of one command per
 * changed field.
 *
 * - Entries for elements staged for deletion are dropped.
 * - `id` entries (and `from`/`to` entries for links) are dropped for every
 *   element: for freshly created (temp-id) elements they are create-only and
 *   were consumed by the create call; for persisted elements renames and
 *   endpoint changes are unsupported — the backend patch handler has no
 *   `id`/`from`/`to` case, so emitting them could only fail the save.
 * - Entries for freshly created (temp-id) elements are re-targeted to the
 *   real id assigned at creation.
 * - Temp-id entries whose element failed to create (no real id) are dropped.
 */
export function buildFieldPatches(args: {
  draftEntries: DraftEntry[];
  pendingDeleteKeys: ReadonlySet<string>;
  tempIdPrefix: string;
  tempToRealId: ReadonlyMap<string, string>;
}): PatchItem[] {
  const { draftEntries, pendingDeleteKeys, tempIdPrefix, tempToRealId } = args;
  const fieldPatches: PatchItem[] = [];
  for (const { kind, id, field, value } of draftEntries) {
    const targetId = id.startsWith(tempIdPrefix) ? tempToRealId.get(id) : id;
    if (pendingDeleteKeys.has(`${kind}:${targetId ?? id}`)) {
      continue;
    }

    // `id` (and `from`/`to` for links) never patches. For temp-id elements
    // these fields are create-only — already consumed by the create call.
    // For persisted elements renames and endpoint changes stay unsupported
    // until designed properly: the backend has no id/from/to patch handler,
    // so emitting them could only fail the save.
    const isUnsupportedField =
      field === "id" ||
      ((kind === "pipe" || kind === "pump" || kind === "valve") &&
        (field === "from" || field === "to"));
    if (isUnsupportedField) {
      continue;
    }

    if (!targetId) continue;
    fieldPatches.push({ kind, id: targetId, field, value });
  }
  return fieldPatches;
}

export async function saveStagedElements(
  args: SaveStagedElementsArgs,
): Promise<SaveStagedElementsResult> {
  const {
    draftEntries,
    pendingAdds,
    pendingDeletes,
    pendingDeleteKeys,
    junctionRowsAll,
    tankRowsAll,
    reservoirRowsAll,
    allElementIds,
    tempIdPrefix,
  } = args;

  const persistedNodeIds = [
    ...junctionRowsAll,
    ...tankRowsAll,
    ...reservoirRowsAll,
  ]
    .map((r) => r.id)
    .filter((id) => !id.startsWith(tempIdPrefix));
  const nodeIdsForLinks = [...persistedNodeIds];
  const usedIds = new Set<string>(allElementIds);
  const tempToRealId = new Map<string, string>();

  const nextId = (prefix: string) => {
    let n = 1;
    while (usedIds.has(`${prefix}${n}`)) n += 1;
    const id = `${prefix}${n}`;
    usedIds.add(id);
    return id;
  };

  const entryValue = (
    kind: ElementKind,
    id: string,
    field: string,
  ): number | string | undefined =>
    draftEntries.find(
      (e) => e.kind === kind && e.id === id && e.field === field,
    )?.value;

  const resolvePendingId = (
    kind: ElementKind,
    tempId: string,
    prefix: string,
  ): string => {
    const requested = String(entryValue(kind, tempId, "id") ?? "").trim();
    if (requested.length === 0) return nextId(prefix);
    if (usedIds.has(requested)) {
      throw new Error(`ID '${requested}' is already in use`);
    }
    usedIds.add(requested);
    return requested;
  };

  const numberValue = (
    v: number | string | undefined,
    fallback: number,
  ): number => {
    const n = typeof v === "number" ? v : Number(v);
    return Number.isFinite(n) ? n : fallback;
  };

  const errorMessage = (err: unknown, fallback: string): string => {
    if (typeof err === "string") return err;
    if (err instanceof Error && err.message.trim().length > 0)
      return err.message;
    if (err && typeof err === "object" && "message" in err) {
      const msg = (err as { message?: unknown }).message;
      if (typeof msg === "string" && msg.trim().length > 0) return msg;
    }
    return fallback;
  };

  let failed = 0;
  let applied = 0;
  const errors: string[] = [];

  for (const pending of pendingAdds.filter(
    (p) => p.kind === "junction" || p.kind === "tank" || p.kind === "reservoir",
  )) {
    try {
      if (pending.kind === "junction") {
        const id = resolvePendingId("junction", pending.tempId, "J");
        const x = numberValue(entryValue("junction", pending.tempId, "x"), 0);
        const y = numberValue(entryValue("junction", pending.tempId, "y"), 0);
        const elevation = numberValue(
          entryValue("junction", pending.tempId, "elevation"),
          0,
        );
        await createNode("junction", id, x, y, elevation);
        tempToRealId.set(pending.tempId, id);
        nodeIdsForLinks.push(id);
        applied++;
      } else if (pending.kind === "tank") {
        const id = resolvePendingId("tank", pending.tempId, "T");
        const x = numberValue(entryValue("tank", pending.tempId, "x"), 0);
        const y = numberValue(entryValue("tank", pending.tempId, "y"), 0);
        const elevation = numberValue(
          entryValue("tank", pending.tempId, "elevation"),
          0,
        );
        const minLevel = numberValue(
          entryValue("tank", pending.tempId, "minLevel"),
          0,
        );
        const maxLevel = numberValue(
          entryValue("tank", pending.tempId, "maxLevel"),
          3,
        );
        const initialLevel = numberValue(
          entryValue("tank", pending.tempId, "initialLevel"),
          1.5,
        );
        await createNode(
          "tank",
          id,
          x,
          y,
          elevation,
          minLevel,
          maxLevel,
          initialLevel,
        );
        tempToRealId.set(pending.tempId, id);
        nodeIdsForLinks.push(id);
        applied++;
      } else {
        const id = resolvePendingId("reservoir", pending.tempId, "R");
        const x = numberValue(entryValue("reservoir", pending.tempId, "x"), 0);
        const y = numberValue(entryValue("reservoir", pending.tempId, "y"), 0);
        const head = numberValue(
          entryValue("reservoir", pending.tempId, "head"),
          0,
        );
        await createNode("reservoir", id, x, y, head);
        tempToRealId.set(pending.tempId, id);
        nodeIdsForLinks.push(id);
        applied++;
      }
    } catch (err) {
      failed++;
      errors.push(errorMessage(err, "Could not create node"));
    }
  }

  for (const pending of pendingAdds.filter(
    (p) => p.kind === "pipe" || p.kind === "pump" || p.kind === "valve",
  )) {
    if (nodeIdsForLinks.length < 2) {
      failed++;
      errors.push("Add at least two nodes before creating links");
      continue;
    }
    const requestedFrom = String(
      entryValue(pending.kind, pending.tempId, "from") ?? "",
    ).trim();
    const requestedTo = String(
      entryValue(pending.kind, pending.tempId, "to") ?? "",
    ).trim();
    const fromId =
      requestedFrom.length > 0 ? requestedFrom : nodeIdsForLinks[0];
    const toId = requestedTo.length > 0 ? requestedTo : nodeIdsForLinks[1];

    if (!nodeIdsForLinks.includes(fromId)) {
      failed++;
      errors.push(`Unknown from-node '${fromId}'`);
      continue;
    }
    if (!nodeIdsForLinks.includes(toId)) {
      failed++;
      errors.push(`Unknown to-node '${toId}'`);
      continue;
    }
    if (fromId === toId) {
      failed++;
      errors.push("Link endpoints must be different nodes");
      continue;
    }

    try {
      if (pending.kind === "pipe") {
        const id = resolvePendingId("pipe", pending.tempId, "P");
        await createLink("pipe", id, fromId, toId);
        tempToRealId.set(pending.tempId, id);
        applied++;
      } else if (pending.kind === "pump") {
        const id = resolvePendingId("pump", pending.tempId, "PU");
        await createLink("pump", id, fromId, toId);
        tempToRealId.set(pending.tempId, id);
        applied++;
      } else {
        const id = resolvePendingId("valve", pending.tempId, "V");
        await createLink("valve", id, fromId, toId);
        tempToRealId.set(pending.tempId, id);
        applied++;
      }
    } catch (err) {
      failed++;
      errors.push(errorMessage(err, "Could not create link"));
    }
  }

  // Collect all field edits and apply them in a single bulk backend call
  // (see buildFieldPatches).
  const fieldPatches = buildFieldPatches({
    draftEntries,
    pendingDeleteKeys,
    tempIdPrefix,
    tempToRealId,
  });
  if (fieldPatches.length > 0) {
    try {
      const result = await patchElements(fieldPatches);
      applied += result.applied;
      failed += result.errors.length;
      errors.push(...result.errors);
    } catch (err) {
      failed += fieldPatches.length;
      errors.push(errorMessage(err, "Could not apply changes"));
    }
  }

  for (const { kind, id } of pendingDeletes) {
    try {
      await deleteElement(kind, id);
      applied++;
    } catch (err) {
      failed++;
      errors.push(errorMessage(err, `Could not delete ${kind} '${id}'`));
    }
  }

  return { applied, failed, errors };
}
