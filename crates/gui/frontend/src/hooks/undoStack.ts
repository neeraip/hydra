/**
 * Undo/redo stacks for *committed* network edits (canvas drags, canvas/
 * inspector creates and deletes, and Network Editor element saves), plus the
 * pure inverse-set construction used at capture time.
 *
 * Pure logic only — no IPC. Applying an `EditSet` (which does call the
 * backend mutation wrappers) lives in `useUndoRedo.ts`.
 *
 * Stacks are module-level, keyed per `(projectId, scenarioId)` and bounded
 * at {@link MAX_UNDO_ENTRIES} entries; a `useSyncExternalStore` hook exposes
 * them to React. Switching project or scenario simply addresses a different
 * key, so no explicit clearing is needed on switch. Scenario reloads via
 * `primeNetworkData` re-read the same saved INP the captured entries were
 * built against, so entries stay valid across switches; if an entry ever
 * goes stale (e.g. the INP changed outside the app), applying it fails, the
 * entry is dropped, and the error is toasted (see `useUndoRedo`).
 * NetworkDataContext does not expose its internal full-refetch signal, and
 * structural refetches are triggered by our own captured mutations anyway,
 * so stacks are deliberately NOT cleared on refetch.
 *
 * KNOWN EXCLUSIONS (never captured, cannot be undone here):
 *   - pattern / curve / control / rule edits,
 *   - simulation-parameter (sim-params) changes,
 *   - CRS changes.
 * Additional limitations, documented per helper below:
 *   - recreated links lose polyline vertices and tags (`create_link` cannot
 *     restore them),
 *   - field edits whose previous value cannot be read from the snapshot
 *     (e.g. a null valve setting) are dropped from both sides of the entry.
 */

import { useCallback, useSyncExternalStore } from "react";
import type { Link, Node } from "../types";
import type { NewElement } from "./network";

// ── Types ──────────────────────────────────────────────────────────────────

/** Same shape as `PatchItem` (hooks/network.ts) — one `patch_elements` item.
 *  Values are in the display units the patch API expects (m / L/s / mm). */
export interface FieldPatch {
  kind: string;
  id: string;
  field: string;
  value: number | string;
}

/**
 * Everything needed to rebuild a node/link through the existing
 * `createNode` / `createLink` wrappers plus follow-up field patches for
 * attributes the create commands don't accept (junction baseDemand, tank
 * diameter/volumeCurve, reservoir headPattern, all pipe/pump/valve
 * attributes). Node positions travel through the create args (x/y);
 * position *edits* are inverted as x/y field patches instead.
 */
export type RecreateSpec =
  | {
      elementType: "node";
      /** "junction" | "tank" | "reservoir" */
      kind: string;
      id: string;
      x: number;
      y: number;
      /** Elevation for junctions/tanks, head for reservoirs (m). */
      elevation: number;
      minLevel?: number;
      maxLevel?: number;
      initialLevel?: number;
      patches: FieldPatch[];
    }
  | {
      elementType: "link";
      /** "pipe" | "pump" | "valve" */
      kind: string;
      id: string;
      fromId: string;
      toId: string;
      patches: FieldPatch[];
    };

/** A batch of mutations applied strictly in recreate → patch → delete order. */
/**
 * One editing operation, in the vocabulary the element contract defines
 * (hydra-common §4.5) rather than one engine's commands.
 *
 * This is what an undo entry is made of now. The `recreates`, `patches`
 * and `deletes` beside it are the water-distribution editor's own, kept
 * because its staged save still speaks them; anything captured from a
 * surface that goes through the contract uses these, and works for
 * whichever engine holds the model.
 */
export type ElementOp =
  | { op: "move"; id: string; x: number; y: number }
  // `kind` addresses the element together with its id, where the caller
  // knows it. A water-distribution id names an element only within its
  // family — junction `10` and pipe `10` are two elements — so an undo
  // that carried only the id would be refused, or worse, applied to the
  // other one.
  | {
      op: "set";
      id: string;
      key: string;
      value: number | string;
      kind?: string;
    }
  | { op: "rename"; kind: string; from: string; to: string }
  | { op: "reconnect"; id: string; fromId: string; toId: string }
  | { op: "contents"; kind: string; id: string; rows: number[][] }
  | {
      op: "records";
      id: string;
      set: string;
      rows: Array<Array<number | string | null>>;
      /** See the `set` op — an id is half an address in one engine. */
      kind?: string;
    }
  | { op: "create"; element: NewElement }
  | { op: "remove"; kind: string; id: string };

/**
 * The inverse of one operation, or `null` where there is none.
 *
 * `before` is what the model held — a position, or an attribute value —
 * read before the operation was applied. A caller with nothing to read
 * (an element that did not exist) has no inverse to offer, and an entry
 * that cannot be inverted is not captured at all rather than captured
 * and refused later.
 *
 * A removal has no inverse here. Recreating the element is expressible,
 * but a drainage removal also takes records that are not elements — an
 * inflow, a treatment — and this vocabulary cannot put those back. An
 * undo that silently restored less than it removed would be worse than
 * none, so removal clears the history instead.
 */
export function inverseOp(
  op: ElementOp,
  before?: {
    x?: number;
    y?: number;
    value?: number | string;
    fromId?: string;
    toId?: string;
    rows?: number[][];
    records?: Array<Array<number | string | null>>;
  },
): ElementOp | null {
  switch (op.op) {
    case "move":
      return before?.x == null || before?.y == null
        ? null
        : { op: "move", id: op.id, x: before.x, y: before.y };
    case "set":
      return before?.value == null
        ? null
        : {
            op: "set",
            id: op.id,
            key: op.key,
            value: before.value,
            kind: op.kind,
          };
    case "rename":
      return { op: "rename", kind: op.kind, from: op.to, to: op.from };
    case "records":
      // Same shape as contents, and for the same reason: the set is
      // replaced whole, so the rows that were there are the whole of
      // what an undo restores.
      return before?.records == null
        ? null
        : {
            op: "records",
            id: op.id,
            set: op.set,
            rows: before.records,
            kind: op.kind,
          };
    case "contents":
      // The whole table is its own inverse with the previous rows in it,
      // which is the reason the write takes all of them: a sequence of
      // per-row operations could not be reversed without replaying each
      // one, and half of them are illegal on their own.
      return before?.rows == null
        ? null
        : { op: "contents", kind: op.kind, id: op.id, rows: before.rows };
    case "reconnect":
      // Its own inverse shape, and the reason both ends travel together:
      // the pair that was there before is the whole of what to restore,
      // with nothing to read from the model at undo time.
      return before?.fromId == null || before?.toId == null
        ? null
        : {
            op: "reconnect",
            id: op.id,
            fromId: before.fromId,
            toId: before.toId,
          };
    case "create":
      return { op: "remove", kind: op.element.kind, id: op.element.id };
    case "remove":
      return null;
  }
}

export interface EditSet {
  recreates?: RecreateSpec[];
  patches?: FieldPatch[];
  deletes?: Array<{ kind: string; id: string }>;
  /** Contract operations, applied in order before the legacy sets. */
  ops?: ElementOp[];
}

export interface UndoEntry {
  label: string;
  /**
   * Which element the entry is about, where the capturing surface knows.
   *
   * Carried as data rather than folded into `label` because a history
   * that says "Changed invert on 9" has named half an element: an id is
   * unique only within its class, so a junction `9` and a conduit `9`
   * are two different things that share a name. A reader deciding
   * whether to undo needs the kind, and the interface shows a kind with
   * its glyph rather than in words.
   */
  subject?: { kind: string; id: string };
  undo: EditSet;
  redo: EditSet;
}

/**
 * A move, both ways.
 *
 * `null` when there is nowhere to go back to — a caller that could not
 * read the position before the patch has no inverse to offer, and an
 * entry that cannot be reversed is better absent than captured and
 * refused later.
 *
 * Here rather than at either call site because a move happens on two
 * surfaces: dragged on the canvas, typed into the Editor's X and Y
 * columns. It is one operation, and being undoable on one and not the
 * other is a difference a reader discovers by losing work — which is how
 * the Editor's shipped.
 *
 * Expressed in the contract's own vocabulary, so the entry is applied by
 * the same command that made the change and works for whichever engine
 * holds the model. It used to travel as water-distribution field
 * patches, which a drainage model accepted into the history and refused
 * on apply.
 */
export function moveEntry(
  id: string,
  before: readonly [number, number] | null | undefined,
  x: number,
  y: number,
  kind?: string,
): UndoEntry | null {
  if (!before) return null;
  return {
    label: `Moved ${id}`,
    subject: kind ? { kind, id } : undefined,
    undo: { ops: [{ op: "move", id, x: before[0], y: before[1] }] },
    redo: { ops: [{ op: "move", id, x, y }] },
  };
}

export interface UndoStacks {
  undo: readonly UndoEntry[];
  redo: readonly UndoEntry[];
}

// ── Store ──────────────────────────────────────────────────────────────────

export const MAX_UNDO_ENTRIES = 50;

const EMPTY_STACKS: UndoStacks = { undo: [], redo: [] };
const stacksByKey = new Map<string, UndoStacks>();
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

function setStacks(key: string, next: UndoStacks): void {
  if (next.undo.length === 0 && next.redo.length === 0) {
    stacksByKey.delete(key);
  } else {
    stacksByKey.set(key, next);
  }
  emit();
}

/** Stack address for a project/scenario pair (`null` scenario = base model). */
export function stackKey(projectId: string, scenarioId: string | null): string {
  return `${projectId} ${scenarioId ?? ""}`;
}

export function getUndoStacks(key: string): UndoStacks {
  return stacksByKey.get(key) ?? EMPTY_STACKS;
}

/** Record a new entry: pushes onto the undo stack (bounded) and clears the
 *  redo stack — a fresh capture invalidates any redo branch. */
export function pushUndoEntry(key: string, entry: UndoEntry): void {
  const cur = getUndoStacks(key);
  setStacks(key, {
    undo: [...cur.undo, entry].slice(-MAX_UNDO_ENTRIES),
    redo: [],
  });
}

/** Pop the newest undo entry (removing it). Returns `null` when empty. */
export function takeUndo(key: string): UndoEntry | null {
  const cur = getUndoStacks(key);
  const entry = cur.undo[cur.undo.length - 1];
  if (!entry) return null;
  setStacks(key, { undo: cur.undo.slice(0, -1), redo: cur.redo });
  return entry;
}

/** Pop the newest redo entry (removing it). Returns `null` when empty. */
export function takeRedo(key: string): UndoEntry | null {
  const cur = getUndoStacks(key);
  const entry = cur.redo[cur.redo.length - 1];
  if (!entry) return null;
  setStacks(key, { undo: cur.undo, redo: cur.redo.slice(0, -1) });
  return entry;
}

/** After a successful undo apply: park the entry on the redo stack. */
export function pushRedoEntry(key: string, entry: UndoEntry): void {
  const cur = getUndoStacks(key);
  setStacks(key, {
    undo: cur.undo,
    redo: [...cur.redo, entry].slice(-MAX_UNDO_ENTRIES),
  });
}

/** After a successful redo apply: put the entry back on the undo stack
 *  WITHOUT clearing the remaining redo entries (a redo is not a new edit). */
export function restoreUndoEntry(key: string, entry: UndoEntry): void {
  const cur = getUndoStacks(key);
  setStacks(key, {
    undo: [...cur.undo, entry].slice(-MAX_UNDO_ENTRIES),
    redo: cur.redo,
  });
}

/** Drop the redo branch (used when the network mutated but the mutation
 *  could not be captured cleanly, e.g. a partially failed editor save). */
export function clearRedo(key: string): void {
  const cur = getUndoStacks(key);
  if (cur.redo.length === 0) return;
  setStacks(key, { undo: cur.undo, redo: [] });
}

export function clearStacks(key: string): void {
  if (!stacksByKey.has(key)) return;
  stacksByKey.delete(key);
  emit();
}

/** Test seam: wipe every key. */
export function clearAllStacks(): void {
  if (stacksByKey.size === 0) return;
  stacksByKey.clear();
  emit();
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** Live view of the stacks for the given project/scenario. */
export function useUndoStacks(
  projectId: string | null,
  scenarioId: string | null,
): UndoStacks {
  const key = projectId ? stackKey(projectId, scenarioId) : null;
  const getSnapshot = useCallback(
    () => (key ? getUndoStacks(key) : EMPTY_STACKS),
    [key],
  );
  return useSyncExternalStore(subscribe, getSnapshot);
}

// ── Inverse-set construction (pure) ────────────────────────────────────────

/** Structural mirror of DraftContext's `DraftEntry`. */
export interface StagedFieldEntry {
  kind: string;
  id: string;
  field: string;
  value: number | string;
}
/** Structural mirror of DraftContext's `PendingAdd`. */
export interface StagedAdd {
  kind: string;
  tempId: string;
}
/** Structural mirror of DraftContext's `PendingDelete`. */
export interface StagedDelete {
  kind: string;
  id: string;
}

const NODE_KINDS: ReadonlySet<string> = new Set([
  "junction",
  "tank",
  "reservoir",
]);

/**
 * The field patch that restores an element's *current* committed value for
 * `field`, read from the snapshot maps. Returns `null` when the previous
 * value cannot be represented through the patch API (see module header) —
 * callers drop such pairs from both sides of the entry.
 *
 * The pump `curve` / `powerKw` pair is mutually exclusive on the backend
 * (setting one clears the other), so inverting either field restores
 * whichever of the two the pump currently carries.
 */
export function inverseFieldPatch(
  kind: string,
  id: string,
  field: string,
  nodesById: ReadonlyMap<string, Node>,
  linksById: ReadonlyMap<string, Link>,
): FieldPatch | null {
  const patch = (
    value: number | string | null,
    f = field,
  ): FieldPatch | null =>
    value === null ? null : { kind, id, field: f, value };

  if (NODE_KINDS.has(kind)) {
    const n = nodesById.get(id);
    if (!n) return null;
    switch (field) {
      case "elevation":
      case "head":
        return patch(n.elevation ?? 0);
      case "baseDemand":
        return patch(n.baseDemand ?? 0);
      case "x":
        return patch(n.x);
      case "y":
        return patch(n.y);
      case "pattern":
      case "headPattern":
        return patch(n.headPattern ?? "");
      case "minLevel":
        return patch(n.tankMinLevel ?? 0);
      case "maxLevel":
        return patch(n.tankMaxLevel ?? 0);
      case "initialLevel":
        return patch(n.tankInitialLevel ?? 0);
      case "diameter":
        return patch(n.tankDiameter ?? null);
      case "volumeCurve":
        return patch(n.tankVolumeCurve ?? "");
      default:
        return null;
    }
  }

  const l = linksById.get(id);
  if (!l) return null;
  if (kind === "pipe") {
    switch (field) {
      case "length":
        return patch(l.length ?? 0);
      case "diameter":
        return patch(l.diameter ?? 0);
      case "roughness":
        return patch(l.roughness ?? 0);
      case "status":
        // The patch API accepts open/closed/cv (case-insensitive); only an
        // unknown/pre-v3 status (field absent) is uninvertible.
        return l.initialStatus === "open" ||
          l.initialStatus === "closed" ||
          l.initialStatus === "cv"
          ? patch(l.initialStatus)
          : null;
      default:
        return null;
    }
  }
  if (kind === "pump") {
    switch (field) {
      case "speed":
        return patch(l.pumpSpeed ?? null);
      case "curve":
      case "powerKw":
        if (l.pumpCurve) return patch(l.pumpCurve, "curve");
        if (l.pumpPowerKw != null) return patch(l.pumpPowerKw, "powerKw");
        return null;
      default:
        return null;
    }
  }
  if (kind === "valve") {
    switch (field) {
      case "diameter":
        return patch(l.diameter ?? 0);
      case "valveType":
        return patch(l.valveType ?? null);
      case "setting":
      case "valveSetting":
        return patch(l.valveSetting ?? null);
      case "valveCurve":
        return patch(l.valveCurve ?? "");
      default:
        return null;
    }
  }
  return null;
}

/**
 * RecreateSpec for a node captured from the committed snapshot. Attributes
 * `create_node` accepts travel as create args; the rest become follow-up
 * patches. Not captured (lost on recreate): tags, demand patterns, emitter
 * coefficients — attributes the frontend DTO doesn't carry.
 */
export function recreateSpecForNode(n: Node): RecreateSpec {
  const patches: FieldPatch[] = [];
  if (n.type === "junction" && typeof n.baseDemand === "number") {
    if (n.baseDemand !== 0) {
      patches.push({
        kind: "junction",
        id: n.id,
        field: "baseDemand",
        value: n.baseDemand,
      });
    }
  }
  if (n.type === "tank") {
    if (n.tankDiameter != null) {
      patches.push({
        kind: "tank",
        id: n.id,
        field: "diameter",
        value: n.tankDiameter,
      });
    }
    if (n.tankVolumeCurve) {
      patches.push({
        kind: "tank",
        id: n.id,
        field: "volumeCurve",
        value: n.tankVolumeCurve,
      });
    }
  }
  if (n.type === "reservoir" && n.headPattern) {
    patches.push({
      kind: "reservoir",
      id: n.id,
      field: "headPattern",
      value: n.headPattern,
    });
  }
  const spec: RecreateSpec = {
    elementType: "node",
    kind: n.type,
    id: n.id,
    x: n.x,
    y: n.y,
    elevation: n.elevation ?? 0,
    patches,
  };
  if (n.type === "tank") {
    if (n.tankMinLevel != null) spec.minLevel = n.tankMinLevel;
    if (n.tankMaxLevel != null) spec.maxLevel = n.tankMaxLevel;
    if (n.tankInitialLevel != null) spec.initialLevel = n.tankInitialLevel;
  }
  return spec;
}

/**
 * RecreateSpec for a link captured from the committed snapshot. Limitations:
 * polyline vertices and tags cannot be restored (no create arg / patch field
 * exists for them).
 */
export function recreateSpecForLink(l: Link): RecreateSpec {
  const patches: FieldPatch[] = [];
  if (l.type === "pipe") {
    if (typeof l.length === "number") {
      patches.push({
        kind: "pipe",
        id: l.id,
        field: "length",
        value: l.length,
      });
    }
    patches.push({
      kind: "pipe",
      id: l.id,
      field: "diameter",
      value: l.diameter ?? 0,
    });
    if (typeof l.roughness === "number") {
      patches.push({
        kind: "pipe",
        id: l.id,
        field: "roughness",
        value: l.roughness,
      });
    }
    // create_link defaults to Open; a closed or check-valve pipe needs a
    // follow-up status patch (the patch API accepts open/closed/cv).
    if (l.initialStatus === "closed" || l.initialStatus === "cv") {
      patches.push({
        kind: "pipe",
        id: l.id,
        field: "status",
        value: l.initialStatus,
      });
    }
  } else if (l.type === "pump") {
    // curve and constant power are mutually exclusive — restore whichever
    // the pump carried (patching one clears the other on the backend).
    if (l.pumpCurve) {
      patches.push({
        kind: "pump",
        id: l.id,
        field: "curve",
        value: l.pumpCurve,
      });
    } else if (l.pumpPowerKw != null) {
      patches.push({
        kind: "pump",
        id: l.id,
        field: "powerKw",
        value: l.pumpPowerKw,
      });
    }
    if (l.pumpSpeed != null) {
      patches.push({
        kind: "pump",
        id: l.id,
        field: "speed",
        value: l.pumpSpeed,
      });
    }
  } else {
    // valveType first: valveSetting's unit conversion depends on it.
    if (l.valveType) {
      patches.push({
        kind: "valve",
        id: l.id,
        field: "valveType",
        value: l.valveType,
      });
    }
    patches.push({
      kind: "valve",
      id: l.id,
      field: "diameter",
      value: l.diameter ?? 0,
    });
    if (l.valveSetting != null) {
      patches.push({
        kind: "valve",
        id: l.id,
        field: "valveSetting",
        value: l.valveSetting,
      });
    }
    if (l.valveCurve) {
      patches.push({
        kind: "valve",
        id: l.id,
        field: "valveCurve",
        value: l.valveCurve,
      });
    }
  }
  return {
    elementType: "link",
    kind: l.type,
    id: l.id,
    fromId: l.fromId,
    toId: l.toId,
    patches,
  };
}

/**
 * RecreateSpecs that rebuild `id` (and, for a node, every link that will
 * cascade-delete with it) from the committed snapshot. Node spec first so
 * link recreates find their endpoint. Returns `null` when the element is
 * not in the snapshot.
 */
export function recreateSpecsForDelete(
  kind: string,
  id: string,
  nodes: readonly Node[],
  links: readonly Link[],
): RecreateSpec[] | null {
  if (NODE_KINDS.has(kind)) {
    const n = nodes.find((x) => x.id === id);
    if (!n) return null;
    const attached = links.filter((l) => l.fromId === id || l.toId === id);
    return [recreateSpecForNode(n), ...attached.map(recreateSpecForLink)];
  }
  const l = links.find((x) => x.id === id);
  if (!l) return null;
  return [recreateSpecForLink(l)];
}
