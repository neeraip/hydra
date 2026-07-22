/**
 * Unified draft store for the Network Editor's four tabs (Elements, Pump
 * curves, Patterns, Controls). One amalgamated set of pending changes spans
 * all four tabs — switching tabs never loses progress, because this
 * provider lives above the tab switcher in `NetworkEditor.tsx` and is never
 * unmounted while a project is open.
 *
 * Every field edit across all four editors writes into this store
 * immediately (matching how `EditableCell` already stages Elements edits);
 * nothing reaches the backend until `saveAll()` is called. `discardAll()`
 * clears every pending structure.
 *
 * Curves and patterns have real, user-chosen IDs, so pending creates are
 * keyed directly by that ID. Controls and rules have no natural ID in the
 * INP format — pending creates use a `tmp-N` key, and existing rows use a
 * `idx-N` key (their current backend array position), stable for the
 * lifetime of the draft since nothing commits until `saveAll()`.
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
} from "react";
import { useActiveProject, useAppState } from "../AppContext";
import {
  buildPreviewPatches,
  type DraftEntry,
  type ElementKind,
  type PendingAdd,
  type PendingDelete,
} from "../pages/project/NetworkEditor/elementsEditorDerivations";
import { saveStagedElements } from "../pages/project/NetworkEditor/elementsEditorSave";
import {
  type CurvePoint,
  createControl,
  createCurve,
  createPattern,
  createRule,
  deleteControl,
  deleteCurve,
  deletePattern,
  deleteRule,
  type PatchItem,
  type PumpCurve,
  type RuleDto,
  type SimpleControlDto,
  saveProjectOnDisk,
  type TimePattern,
  updateControl,
  updateCurvePoints,
  updatePatternMultipliers,
  updateRule,
  useJunctionRows,
  usePipeRows,
  usePumpRows,
  useReservoirRows,
  useTankRows,
  useValveRows,
} from "./index";
import { useNetworkVersion } from "./NetworkVersionContext";

// ── Elements (types re-exported from elementsEditorDerivations) ──────────────

export type { DraftEntry, ElementKind, PendingAdd, PendingDelete };

export const ELEMENT_TEMP_ID_PREFIX = "__new__:";

// ── Save result ────────────────────────────────────────────────────────────────

export interface SaveAllResult {
  applied: number;
  failed: number;
  errors: string[];
}

interface DraftContextValue {
  // Elements
  elementsDraft: Map<string, DraftEntry>;
  setElementsDraft: React.Dispatch<
    React.SetStateAction<Map<string, DraftEntry>>
  >;
  pendingAdds: PendingAdd[];
  setPendingAdds: React.Dispatch<React.SetStateAction<PendingAdd[]>>;
  pendingDeletes: PendingDelete[];
  setPendingDeletes: React.Dispatch<React.SetStateAction<PendingDelete[]>>;
  nextTempIndex: React.RefObject<number>;

  // Curves — keyed by curve ID. `curveAdds` holds not-yet-created curves
  // (key = the chosen ID); `curveEdits` holds staged point edits for
  // existing curves.
  curveAdds: Map<string, CurvePoint[]>;
  setCurveAdds: React.Dispatch<React.SetStateAction<Map<string, CurvePoint[]>>>;
  curveEdits: Map<string, CurvePoint[]>;
  setCurveEdits: React.Dispatch<
    React.SetStateAction<Map<string, CurvePoint[]>>
  >;
  curveDeletes: Set<string>;
  setCurveDeletes: React.Dispatch<React.SetStateAction<Set<string>>>;

  // Patterns — same shape as curves.
  patternAdds: Map<string, number[]>;
  setPatternAdds: React.Dispatch<React.SetStateAction<Map<string, number[]>>>;
  patternEdits: Map<string, number[]>;
  setPatternEdits: React.Dispatch<React.SetStateAction<Map<string, number[]>>>;
  patternDeletes: Set<string>;
  setPatternDeletes: React.Dispatch<React.SetStateAction<Set<string>>>;

  // Controls — keyed by `idx-${originalIndex}` (existing) or `tmp-${n}` (new).
  controlAdds: Map<string, SimpleControlDto>;
  setControlAdds: React.Dispatch<
    React.SetStateAction<Map<string, SimpleControlDto>>
  >;
  controlEdits: Map<string, SimpleControlDto>;
  setControlEdits: React.Dispatch<
    React.SetStateAction<Map<string, SimpleControlDto>>
  >;
  controlDeletes: Set<string>;
  setControlDeletes: React.Dispatch<React.SetStateAction<Set<string>>>;

  // Rules — same keying scheme as controls.
  ruleAdds: Map<string, RuleDto>;
  setRuleAdds: React.Dispatch<React.SetStateAction<Map<string, RuleDto>>>;
  ruleEdits: Map<string, RuleDto>;
  setRuleEdits: React.Dispatch<React.SetStateAction<Map<string, RuleDto>>>;
  ruleDeletes: Set<string>;
  setRuleDeletes: React.Dispatch<React.SetStateAction<Set<string>>>;

  /** Generates a unique key like `tmp-3` for a given domain prefix. */
  nextTempKey: (prefix: string) => string;

  /** Total pending-change count across all four tabs. */
  dirtyCount: number;
  /** Per-tab pending-change counts, for the section-rail dots. */
  dirtyBySection: {
    elements: number;
    curves: number;
    patterns: number;
    controls: number;
  };
  /** Combined preview of every staged change, grouped by kind. */
  previewPatches: PatchItem[];

  discardAll: () => void;
  saveAll: () => Promise<SaveAllResult>;
}

const Ctx = createContext<DraftContextValue | null>(null);

export function DraftProvider({ children }: { children: ReactNode }) {
  const { showToast, activeScenarioId } = useAppState();
  const { project } = useActiveProject();
  const { bumpNetwork, markEdited } = useNetworkVersion();

  // Base data needed for save orchestration (element ID pools, cascade
  // detection).
  const junctionRowsAll = useJunctionRows();
  const tankRowsAll = useTankRows();
  const reservoirRowsAll = useReservoirRows();
  const pipeRowsAll = usePipeRows();
  const pumpRowsAll = usePumpRows();
  const valveRowsAll = useValveRows();

  const allElementIds = useMemo(
    () =>
      new Set<string>([
        ...junctionRowsAll.map((r) => r.id),
        ...tankRowsAll.map((r) => r.id),
        ...reservoirRowsAll.map((r) => r.id),
        ...pipeRowsAll.map((r) => r.id),
        ...pumpRowsAll.map((r) => r.id),
        ...valveRowsAll.map((r) => r.id),
      ]),
    [
      junctionRowsAll,
      tankRowsAll,
      reservoirRowsAll,
      pipeRowsAll,
      pumpRowsAll,
      valveRowsAll,
    ],
  );

  // Elements
  const [elementsDraft, setElementsDraft] = useState<Map<string, DraftEntry>>(
    () => new Map(),
  );
  const [pendingAdds, setPendingAdds] = useState<PendingAdd[]>([]);
  const [pendingDeletes, setPendingDeletes] = useState<PendingDelete[]>([]);
  const nextTempIndex = useRef(1);

  // Curves
  const [curveAdds, setCurveAdds] = useState<Map<string, CurvePoint[]>>(
    () => new Map(),
  );
  const [curveEdits, setCurveEdits] = useState<Map<string, CurvePoint[]>>(
    () => new Map(),
  );
  const [curveDeletes, setCurveDeletes] = useState<Set<string>>(
    () => new Set(),
  );

  // Patterns
  const [patternAdds, setPatternAdds] = useState<Map<string, number[]>>(
    () => new Map(),
  );
  const [patternEdits, setPatternEdits] = useState<Map<string, number[]>>(
    () => new Map(),
  );
  const [patternDeletes, setPatternDeletes] = useState<Set<string>>(
    () => new Set(),
  );

  // Controls
  const [controlAdds, setControlAdds] = useState<Map<string, SimpleControlDto>>(
    () => new Map(),
  );
  const [controlEdits, setControlEdits] = useState<
    Map<string, SimpleControlDto>
  >(() => new Map());
  const [controlDeletes, setControlDeletes] = useState<Set<string>>(
    () => new Set(),
  );

  // Rules
  const [ruleAdds, setRuleAdds] = useState<Map<string, RuleDto>>(
    () => new Map(),
  );
  const [ruleEdits, setRuleEdits] = useState<Map<string, RuleDto>>(
    () => new Map(),
  );
  const [ruleDeletes, setRuleDeletes] = useState<Set<string>>(() => new Set());

  const tempCounters = useRef<Record<string, number>>({});
  const nextTempKey = useCallback((prefix: string) => {
    const n = (tempCounters.current[prefix] ?? 0) + 1;
    tempCounters.current[prefix] = n;
    return `${prefix}${n}`;
  }, []);

  const elementsDirtyCount =
    elementsDraft.size + pendingAdds.length + pendingDeletes.length;
  const curvesDirtyCount = curveAdds.size + curveEdits.size + curveDeletes.size;
  const patternsDirtyCount =
    patternAdds.size + patternEdits.size + patternDeletes.size;
  const controlsDirtyCount =
    controlAdds.size +
    controlEdits.size +
    controlDeletes.size +
    ruleAdds.size +
    ruleEdits.size +
    ruleDeletes.size;
  const dirtyCount =
    elementsDirtyCount +
    curvesDirtyCount +
    patternsDirtyCount +
    controlsDirtyCount;

  const dirtyBySection = {
    elements: elementsDirtyCount,
    curves: curvesDirtyCount,
    patterns: patternsDirtyCount,
    controls: controlsDirtyCount,
  };

  const previewPatches = useMemo<PatchItem[]>(() => {
    const items: PatchItem[] = buildPreviewPatches({
      draftEntries: Array.from(elementsDraft.values()),
      pendingAdds,
      pendingDeletes,
      pendingDeleteKeys: new Set(
        pendingDeletes.map((d) => `${d.kind}:${d.id}`),
      ),
      pipeRowsAll,
      pumpRowsAll,
      valveRowsAll,
      tempIdPrefix: ELEMENT_TEMP_ID_PREFIX,
    });
    for (const [id, points] of curveAdds) {
      items.push({
        kind: "curve",
        id,
        field: "create",
        value: `${points.length} points`,
      });
    }
    for (const [id, points] of curveEdits) {
      items.push({
        kind: "curve",
        id,
        field: "points",
        value: `${points.length} points`,
      });
    }
    for (const id of curveDeletes) {
      items.push({
        kind: "curve",
        id,
        field: "delete",
        value: "(delete curve)",
      });
    }
    for (const [id, m] of patternAdds) {
      items.push({
        kind: "pattern",
        id,
        field: "create",
        value: `${m.length} multipliers`,
      });
    }
    for (const [id, m] of patternEdits) {
      items.push({
        kind: "pattern",
        id,
        field: "multipliers",
        value: `${m.length} multipliers`,
      });
    }
    for (const id of patternDeletes) {
      items.push({
        kind: "pattern",
        id,
        field: "delete",
        value: "(delete pattern)",
      });
    }
    for (const [key, c] of controlAdds) {
      items.push({
        kind: "control",
        id: key,
        field: "create",
        value: c.linkId,
      });
    }
    for (const [key, c] of controlEdits) {
      items.push({
        kind: "control",
        id: key,
        field: "update",
        value: c.linkId,
      });
    }
    for (const key of controlDeletes) {
      items.push({
        kind: "control",
        id: key,
        field: "delete",
        value: "(delete control)",
      });
    }
    for (const [key, r] of ruleAdds) {
      items.push({
        kind: "rule",
        id: key,
        field: "create",
        value: r.name || key,
      });
    }
    for (const [key, r] of ruleEdits) {
      items.push({
        kind: "rule",
        id: key,
        field: "update",
        value: r.name || key,
      });
    }
    for (const key of ruleDeletes) {
      items.push({
        kind: "rule",
        id: key,
        field: "delete",
        value: "(delete rule)",
      });
    }
    return items;
  }, [
    elementsDraft,
    pendingAdds,
    pendingDeletes,
    pipeRowsAll,
    pumpRowsAll,
    valveRowsAll,
    curveAdds,
    curveEdits,
    curveDeletes,
    patternAdds,
    patternEdits,
    patternDeletes,
    controlAdds,
    controlEdits,
    controlDeletes,
    ruleAdds,
    ruleEdits,
    ruleDeletes,
  ]);

  const discardAll = useCallback(() => {
    setElementsDraft(new Map());
    setPendingAdds([]);
    setPendingDeletes([]);
    setCurveAdds(new Map());
    setCurveEdits(new Map());
    setCurveDeletes(new Set());
    setPatternAdds(new Map());
    setPatternEdits(new Map());
    setPatternDeletes(new Set());
    setControlAdds(new Map());
    setControlEdits(new Map());
    setControlDeletes(new Set());
    setRuleAdds(new Map());
    setRuleEdits(new Map());
    setRuleDeletes(new Set());
  }, []);

  const saveAll = useCallback(async (): Promise<SaveAllResult> => {
    let applied = 0;
    let failed = 0;
    const errors: string[] = [];

    const record = async (label: string, fn: () => Promise<void>) => {
      try {
        await fn();
        applied++;
      } catch (err) {
        failed++;
        errors.push(typeof err === "string" ? err : `Could not ${label}`);
      }
    };

    // ── Elements (creates → field patches → deletes) ──────────────────────
    if (elementsDirtyCount > 0) {
      const result = await saveStagedElements({
        draftEntries: Array.from(elementsDraft.values()),
        pendingAdds,
        pendingDeletes,
        pendingDeleteKeys: new Set(
          pendingDeletes.map((d) => `${d.kind}:${d.id}`),
        ),
        junctionRowsAll,
        tankRowsAll,
        reservoirRowsAll,
        allElementIds,
        tempIdPrefix: ELEMENT_TEMP_ID_PREFIX,
      });
      applied += result.applied;
      failed += result.failed;
      errors.push(...result.errors);
    }

    // ── Curves: edits → deletes → creates ─────────────────────────────────
    for (const [id, points] of curveEdits) {
      await record(`update curve '${id}'`, () =>
        updateCurvePoints(
          id,
          points.map((p) => p.flow),
          points.map((p) => p.head),
        ),
      );
    }
    for (const id of curveDeletes) {
      await record(`delete curve '${id}'`, () => deleteCurve(id));
    }
    for (const [id, points] of curveAdds) {
      await record(`create curve '${id}'`, async () => {
        await createCurve(id);
        await updateCurvePoints(
          id,
          points.map((p) => p.flow),
          points.map((p) => p.head),
        );
      });
    }

    // ── Patterns: edits → deletes → creates ───────────────────────────────
    for (const [id, m] of patternEdits) {
      await record(`update pattern '${id}'`, () =>
        updatePatternMultipliers(id, m),
      );
    }
    for (const id of patternDeletes) {
      await record(`delete pattern '${id}'`, () => deletePattern(id));
    }
    for (const [id, m] of patternAdds) {
      await record(`create pattern '${id}'`, async () => {
        await createPattern(id);
        await updatePatternMultipliers(id, m);
      });
    }

    // ── Controls: edits → deletes (descending index) → creates ───────────
    for (const [key, dto] of controlEdits) {
      const idx = parseInt(key.replace("idx-", ""), 10);
      await record(`update control ${idx}`, () => updateControl(idx, dto));
    }
    const controlDeleteIndices = Array.from(controlDeletes)
      .map((k) => parseInt(k.replace("idx-", ""), 10))
      .sort((a, b) => b - a);
    for (const idx of controlDeleteIndices) {
      await record(`delete control ${idx}`, () => deleteControl(idx));
    }
    for (const [, dto] of controlAdds) {
      await record("create control", () => createControl(dto));
    }

    // ── Rules: edits → deletes (descending index) → creates ──────────────
    for (const [key, dto] of ruleEdits) {
      const idx = parseInt(key.replace("idx-", ""), 10);
      await record(`update rule ${idx}`, () => updateRule(idx, dto));
    }
    const ruleDeleteIndices = Array.from(ruleDeletes)
      .map((k) => parseInt(k.replace("idx-", ""), 10))
      .sort((a, b) => b - a);
    for (const idx of ruleDeleteIndices) {
      await record(`delete rule ${idx}`, () => deleteRule(idx));
    }
    for (const [, dto] of ruleAdds) {
      await record("create rule", () => createRule(dto));
    }

    if (applied > 0) {
      bumpNetwork();
      if (project?.id) {
        try {
          await saveProjectOnDisk(project.id, activeScenarioId);
          markEdited(activeScenarioId ?? null);
        } catch {
          // Non-fatal: in-memory network state is already correct; the next
          // successful save (or app action that persists) will catch up.
        }
      }
    }

    if (failed > 0) {
      const detail = errors.length > 0 ? `: ${errors[0]}` : "";
      showToast(
        `${failed} change${failed > 1 ? "s" : ""} could not be saved${detail}`,
        "error",
      );
    }

    // Clear whatever succeeded. On partial failure we clear everything
    // rather than retry, since retrying against already-applied mutations
    // (e.g. renamed/created IDs) can no longer be addressed the same way.
    if (applied > 0 || failed === 0) {
      discardAll();
    }

    return { applied, failed, errors };
  }, [
    elementsDirtyCount,
    elementsDraft,
    pendingAdds,
    pendingDeletes,
    junctionRowsAll,
    tankRowsAll,
    reservoirRowsAll,
    allElementIds,
    curveAdds,
    curveEdits,
    curveDeletes,
    patternAdds,
    patternEdits,
    patternDeletes,
    controlAdds,
    controlEdits,
    controlDeletes,
    ruleAdds,
    ruleEdits,
    ruleDeletes,
    bumpNetwork,
    markEdited,
    project,
    activeScenarioId,
    showToast,
    discardAll,
  ]);

  const value: DraftContextValue = {
    elementsDraft,
    setElementsDraft,
    pendingAdds,
    setPendingAdds,
    pendingDeletes,
    setPendingDeletes,
    nextTempIndex,

    curveAdds,
    setCurveAdds,
    curveEdits,
    setCurveEdits,
    curveDeletes,
    setCurveDeletes,

    patternAdds,
    setPatternAdds,
    patternEdits,
    setPatternEdits,
    patternDeletes,
    setPatternDeletes,

    controlAdds,
    setControlAdds,
    controlEdits,
    setControlEdits,
    controlDeletes,
    setControlDeletes,

    ruleAdds,
    setRuleAdds,
    ruleEdits,
    setRuleEdits,
    ruleDeletes,
    setRuleDeletes,

    nextTempKey,
    dirtyCount,
    dirtyBySection,
    previewPatches,
    discardAll,
    saveAll,
  };

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useDraft(): DraftContextValue {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useDraft must be used within DraftProvider");
  return ctx;
}

// Re-exported so consumers of curves/patterns don't need two import sources.
export type { PumpCurve, TimePattern };
