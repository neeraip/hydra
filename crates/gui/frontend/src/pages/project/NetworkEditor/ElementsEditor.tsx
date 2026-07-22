import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useActiveProject, useAppState } from "../../../AppContext";
import { InpDiffModal } from "../../../components/modals/InpDiffModal";
import { TabButton } from "../../../components/ui/TabButton";
import {
  type JunctionRow,
  type PipeRow,
  type PumpRow,
  type ReservoirRow,
  type TankRow,
  useJunctionRows,
  usePipeRows,
  usePumpRows,
  useReservoirRows,
  useTankRows,
  useValveRows,
  type ValveRow,
} from "../../../hooks";
import { useNetworkVersion } from "../../../hooks/NetworkVersionContext";
import {
  buildPreviewPatches,
  collectDirtyKinds,
  type DraftEntry,
  type ElementKind,
  type PendingAdd,
  type PendingDelete,
} from "./elementsEditorDerivations";
import { saveStagedElements } from "./elementsEditorSave";
import { JunctionTable } from "./JunctionTable";
import { PipeTable } from "./PipeTable";
import { PumpTable } from "./PumpTable";
import { ReservoirTable } from "./ReservoirTable";
import { TankTable } from "./TankTable";
import { ValveTable } from "./ValveTable";

type Section =
  | "junctions"
  | "pipes"
  | "pumps"
  | "tanks"
  | "reservoirs"
  | "valves";

const TEMP_ID_PREFIX = "__new__:";

export function ElementsEditor({
  onDraftSizeChange,
  focusPumpId,
  focusPumpToken,
}: {
  onDraftSizeChange?: (n: number) => void;
  /** Pump ID to select when `focusPumpToken` changes (e.g. "attached to" link
   *  clicked from the Pump curves tab). */
  focusPumpId?: string;
  /** Bump this (e.g. `Date.now()`) to re-trigger the jump even for the same id. */
  focusPumpToken?: number;
}) {
  const { showToast, activeScenarioId } = useAppState();
  const { project } = useActiveProject();
  const { bumpNetwork, markEdited } = useNetworkVersion();
  const [previewOpen, setPreviewOpen] = useState(false);
  const junctionRowsAll = useJunctionRows();
  const pipeRowsAll = usePipeRows();
  const pumpRowsAll = usePumpRows();
  const tankRowsAll = useTankRows();
  const reservoirRowsAll = useReservoirRows();
  const valveRowsAll = useValveRows();
  const [activeSection, setActiveSection] = useState<Section>("junctions");
  const [searchQuery, setSearchQuery] = useState("");
  const [sortField, setSortField] = useState<string>("id");
  const [sortAsc, setSortAsc] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pendingAdds, setPendingAdds] = useState<PendingAdd[]>([]);
  const [pendingDeletes, setPendingDeletes] = useState<PendingDelete[]>([]);
  const [draft, setDraft] = useState<Map<string, DraftEntry>>(() => new Map());
  const [discardGen, setDiscardGen] = useState(0);
  const nextTempIndex = useRef(1);
  const tableScrollRef = useRef<HTMLDivElement>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `focusPumpToken` is an intentional trigger to re-run the jump even for the same pump id; `focusPumpId` is read only when a jump fires.
  useEffect(() => {
    if (focusPumpToken == null || !focusPumpId) return;
    setActiveSection("pumps");
    setSelectedId(focusPumpId);
    setSearchQuery("");
  }, [focusPumpToken]);
  const draftValues = useMemo(() => Array.from(draft.values()), [draft]);
  const pendingKeys = useMemo(() => new Set(draft.keys()), [draft]);
  const pendingRowIds = useMemo(
    () => new Set(pendingAdds.map((p) => p.tempId)),
    [pendingAdds],
  );
  const pendingDeleteKeys = useMemo(
    () => new Set(pendingDeletes.map((d) => `${d.kind}:${d.id}`)),
    [pendingDeletes],
  );
  const dirtyCount = draft.size + pendingAdds.length + pendingDeletes.length;
  const previewPatches = useMemo(
    () =>
      buildPreviewPatches({
        draftEntries: draftValues,
        pendingAdds,
        pendingDeletes,
        pendingDeleteKeys,
        pipeRowsAll,
        pumpRowsAll,
        valveRowsAll,
        tempIdPrefix: TEMP_ID_PREFIX,
      }),
    [
      draftValues,
      pendingAdds,
      pendingDeletes,
      pendingDeleteKeys,
      pipeRowsAll,
      pumpRowsAll,
      valveRowsAll,
    ],
  );

  const stagedValue = useCallback(
    (
      kind: ElementKind,
      id: string,
      field: string,
    ): number | string | undefined => {
      return draft.get(`${kind}:${id}:${field}`)?.value;
    },
    [draft],
  );

  // Per-kind dirty flags for the element sub-tabs.
  const dirtyKinds = useMemo(
    () =>
      collectDirtyKinds({
        draftEntries: draftValues,
        pendingAdds,
        pendingDeletes,
      }),
    [draftValues, pendingAdds, pendingDeletes],
  );

  // Notify parent whenever unsaved count changes so the section rail can show a dot.
  const prevDraftSize = useRef(-1);
  if (dirtyCount !== prevDraftSize.current) {
    prevDraftSize.current = dirtyCount;
    onDraftSizeChange?.(dirtyCount);
  }

  // Stage a change locally without writing to the backend yet.
  const handleStage = useCallback(
    (kind: string, id: string, field: string, value: number | string) => {
      setDraft((prev) => {
        const next = new Map(prev);
        next.set(`${kind}:${id}:${field}`, { kind, id, field, value });
        return next;
      });
    },
    [],
  );

  const handleTabClick = (section: Section) => {
    setActiveSection(section);
    setSearchQuery("");
    setSortField("id");
    setSortAsc(true);
    setSelectedId(null);
  };

  const handleSort = (field: string) => {
    if (field === sortField) {
      setSortAsc((prev) => !prev);
    } else {
      setSortField(field);
      setSortAsc(true);
    }
  };

  const q = searchQuery.toLowerCase();

  const filterSort = useCallback(
    <T extends Record<string, unknown>>(rows: T[]): T[] => {
      const filtered = q
        ? rows.filter((r) =>
            Object.values(r).some((v) => String(v).toLowerCase().includes(q)),
          )
        : rows;
      return [...filtered].sort((a, b) => {
        const av = a[sortField];
        const bv = b[sortField];
        if (typeof av === "number" && typeof bv === "number")
          return sortAsc ? av - bv : bv - av;
        return sortAsc
          ? String(av).localeCompare(String(bv))
          : String(bv).localeCompare(String(av));
      });
    },
    [q, sortAsc, sortField],
  );

  const pendingJunctionRows = useMemo<JunctionRow[]>(
    () =>
      pendingAdds
        .filter((p) => p.kind === "junction")
        .map((p) => ({
          id: p.tempId,
          elevation: 0,
          baseDemand: 0,
          demand: 0,
          pressure: null,
          x: 0,
          y: 0,
          belowThreshold: false,
        })),
    [pendingAdds],
  );
  const pendingPipeRows = useMemo<PipeRow[]>(
    () =>
      pendingAdds
        .filter((p) => p.kind === "pipe")
        .map((p) => ({
          id: p.tempId,
          from: String(stagedValue("pipe", p.tempId, "from") ?? ""),
          to: String(stagedValue("pipe", p.tempId, "to") ?? ""),
          length: 0,
          diameter: 0,
          roughness: 100,
          velocity: 0,
          highVelocity: false,
        })),
    [pendingAdds, stagedValue],
  );
  const pendingPumpRows = useMemo<PumpRow[]>(
    () =>
      pendingAdds
        .filter((p) => p.kind === "pump")
        .map((p) => ({
          id: p.tempId,
          from: String(stagedValue("pump", p.tempId, "from") ?? ""),
          to: String(stagedValue("pump", p.tempId, "to") ?? ""),
          curve: null,
          powerKw: null,
          speed: 1,
          velocity: 0,
        })),
    [pendingAdds, stagedValue],
  );
  const pendingTankRows = useMemo<TankRow[]>(
    () =>
      pendingAdds
        .filter((p) => p.kind === "tank")
        .map((p) => ({
          id: p.tempId,
          elevation: 0,
          minLevel: 0,
          maxLevel: 3,
          initialLevel: 1.5,
          diameter: 3,
          volumeCurve: null,
          x: 0,
          y: 0,
        })),
    [pendingAdds],
  );
  const pendingReservoirRows = useMemo<ReservoirRow[]>(
    () =>
      pendingAdds
        .filter((p) => p.kind === "reservoir")
        .map((p) => ({
          id: p.tempId,
          head: 0,
          pattern: null,
          x: 0,
          y: 0,
        })),
    [pendingAdds],
  );
  const pendingValveRows = useMemo<ValveRow[]>(
    () =>
      pendingAdds
        .filter((p) => p.kind === "valve")
        .map((p) => ({
          id: p.tempId,
          from: String(stagedValue("valve", p.tempId, "from") ?? ""),
          to: String(stagedValue("valve", p.tempId, "to") ?? ""),
          valveType: "PRV",
          diameter: 0,
          setting: 0,
          curve: null,
          velocity: 0,
        })),
    [pendingAdds, stagedValue],
  );

  const junctionRowsExisting = useMemo(
    () =>
      junctionRowsAll.filter((r) => !pendingDeleteKeys.has(`junction:${r.id}`)),
    [junctionRowsAll, pendingDeleteKeys],
  );
  const pipeRowsExisting = useMemo(
    () => pipeRowsAll.filter((r) => !pendingDeleteKeys.has(`pipe:${r.id}`)),
    [pipeRowsAll, pendingDeleteKeys],
  );
  const pumpRowsExisting = useMemo(
    () => pumpRowsAll.filter((r) => !pendingDeleteKeys.has(`pump:${r.id}`)),
    [pumpRowsAll, pendingDeleteKeys],
  );
  const tankRowsExisting = useMemo(
    () => tankRowsAll.filter((r) => !pendingDeleteKeys.has(`tank:${r.id}`)),
    [tankRowsAll, pendingDeleteKeys],
  );
  const reservoirRowsExisting = useMemo(
    () =>
      reservoirRowsAll.filter(
        (r) => !pendingDeleteKeys.has(`reservoir:${r.id}`),
      ),
    [reservoirRowsAll, pendingDeleteKeys],
  );
  const valveRowsExisting = useMemo(
    () => valveRowsAll.filter((r) => !pendingDeleteKeys.has(`valve:${r.id}`)),
    [valveRowsAll, pendingDeleteKeys],
  );

  const junctionRowsAllWithPending = useMemo(
    () => [...junctionRowsExisting, ...pendingJunctionRows],
    [junctionRowsExisting, pendingJunctionRows],
  );
  const pipeRowsAllWithPending = useMemo(
    () => [...pipeRowsExisting, ...pendingPipeRows],
    [pipeRowsExisting, pendingPipeRows],
  );
  const pumpRowsAllWithPending = useMemo(
    () => [...pumpRowsExisting, ...pendingPumpRows],
    [pumpRowsExisting, pendingPumpRows],
  );
  const tankRowsAllWithPending = useMemo(
    () => [...tankRowsExisting, ...pendingTankRows],
    [tankRowsExisting, pendingTankRows],
  );
  const reservoirRowsAllWithPending = useMemo(
    () => [...reservoirRowsExisting, ...pendingReservoirRows],
    [reservoirRowsExisting, pendingReservoirRows],
  );
  const valveRowsAllWithPending = useMemo(
    () => [...valveRowsExisting, ...pendingValveRows],
    [valveRowsExisting, pendingValveRows],
  );

  const junctionRows = useMemo(
    () =>
      filterSort(
        junctionRowsAllWithPending as unknown as Record<string, unknown>[],
      ) as unknown as JunctionRow[],
    [junctionRowsAllWithPending, filterSort],
  );
  const pipeRows = useMemo(
    () =>
      filterSort(
        pipeRowsAllWithPending as unknown as Record<string, unknown>[],
      ) as unknown as PipeRow[],
    [pipeRowsAllWithPending, filterSort],
  );
  const pumpRows = useMemo(
    () =>
      filterSort(
        pumpRowsAllWithPending as unknown as Record<string, unknown>[],
      ) as unknown as PumpRow[],
    [pumpRowsAllWithPending, filterSort],
  );
  const tankRows = useMemo(
    () =>
      filterSort(
        tankRowsAllWithPending as unknown as Record<string, unknown>[],
      ) as unknown as TankRow[],
    [tankRowsAllWithPending, filterSort],
  );
  const reservoirRows = useMemo(
    () =>
      filterSort(
        reservoirRowsAllWithPending as unknown as Record<string, unknown>[],
      ) as unknown as ReservoirRow[],
    [reservoirRowsAllWithPending, filterSort],
  );
  const valveRows = useMemo(
    () =>
      filterSort(
        valveRowsAllWithPending as unknown as Record<string, unknown>[],
      ) as unknown as ValveRow[],
    [valveRowsAllWithPending, filterSort],
  );

  const allElementIds = useMemo(() => {
    const ids = new Set<string>();
    junctionRowsAll.forEach((r) => {
      ids.add(r.id);
    });
    pipeRowsAll.forEach((r) => {
      ids.add(r.id);
    });
    pumpRowsAll.forEach((r) => {
      ids.add(r.id);
    });
    tankRowsAll.forEach((r) => {
      ids.add(r.id);
    });
    reservoirRowsAll.forEach((r) => {
      ids.add(r.id);
    });
    valveRowsAll.forEach((r) => {
      ids.add(r.id);
    });
    return ids;
  }, [
    junctionRowsAll,
    pipeRowsAll,
    pumpRowsAll,
    tankRowsAll,
    reservoirRowsAll,
    valveRowsAll,
  ]);

  const nodeReferenceOptions = useMemo(() => {
    const ids = new Set<string>();
    junctionRowsExisting.forEach((r) => {
      ids.add(r.id);
    });
    tankRowsExisting.forEach((r) => {
      ids.add(r.id);
    });
    reservoirRowsExisting.forEach((r) => {
      ids.add(r.id);
    });

    for (const pending of pendingAdds) {
      if (
        pending.kind !== "junction" &&
        pending.kind !== "tank" &&
        pending.kind !== "reservoir"
      )
        continue;
      const requested = String(
        stagedValue(pending.kind, pending.tempId, "id") ?? "",
      ).trim();
      if (requested.length === 0) continue;
      if (requested.includes(" ")) continue;
      ids.add(requested);
    }

    return Array.from(ids).sort((a, b) => a.localeCompare(b));
  }, [
    junctionRowsExisting,
    tankRowsExisting,
    reservoirRowsExisting,
    pendingAdds,
    stagedValue,
  ]);

  const activeKind = useMemo<ElementKind>(() => {
    if (activeSection === "junctions") return "junction";
    if (activeSection === "pipes") return "pipe";
    if (activeSection === "pumps") return "pump";
    if (activeSection === "tanks") return "tank";
    if (activeSection === "valves") return "valve";
    return "reservoir";
  }, [activeSection]);

  const handleAddElement = useCallback(() => {
    const kind: ElementKind = activeKind;
    const tempId = `${TEMP_ID_PREFIX}${kind}_${nextTempIndex.current++}`;
    setPendingAdds((prev) => [...prev, { kind, tempId }]);
    setSearchQuery("");
    setSortField("id");
    setSortAsc(true);
    setSelectedId(tempId);
  }, [activeKind]);

  const handleDeleteSelected = useCallback(() => {
    if (!selectedId) return;
    const id = selectedId;
    const kind = activeKind;

    // Deleting an unsaved row just drops it from local staging.
    if (id.startsWith(TEMP_ID_PREFIX)) {
      setPendingAdds((prev) => prev.filter((p) => p.tempId !== id));
      setDraft((prev) => {
        const next = new Map(prev);
        for (const key of next.keys()) {
          if (key.startsWith(`${kind}:${id}:`)) next.delete(key);
        }
        return next;
      });
      setSelectedId(null);
      return;
    }

    setPendingDeletes((prev) => {
      if (prev.some((d) => d.kind === kind && d.id === id)) return prev;
      return [...prev, { kind, id }];
    });
    setDraft((prev) => {
      const next = new Map(prev);
      for (const [key, value] of next.entries()) {
        if (value.kind === kind && value.id === id) next.delete(key);
      }
      return next;
    });
    setSelectedId(null);
  }, [activeKind, selectedId]);

  // Apply all staged changes/new rows to the backend sequentially, then clear local draft state.
  const handleSave = useCallback(async () => {
    const { applied, failed, errors } = await saveStagedElements({
      draftEntries: draftValues,
      pendingAdds,
      pendingDeletes,
      pendingDeleteKeys,
      junctionRowsAll,
      tankRowsAll,
      reservoirRowsAll,
      allElementIds,
      tempIdPrefix: TEMP_ID_PREFIX,
      projectId: project?.id,
      activeScenarioId,
      markEdited: (scenarioId) => markEdited(scenarioId ?? null),
    });

    if (failed > 0) {
      const detail = errors.length > 0 ? `: ${errors[0]}` : "";
      showToast(
        `${failed} change${failed > 1 ? "s" : ""} could not be saved${detail}`,
      );
    }

    if (failed === 0) {
      setPendingAdds([]);
      setPendingDeletes([]);
      setDraft(new Map());
      bumpNetwork();
      return;
    }

    // If some writes succeeded, reset local draft state to avoid retrying
    // already-applied mutations against changed IDs.
    if (applied > 0) {
      setPendingAdds([]);
      setPendingDeletes([]);
      setDraft(new Map());
      bumpNetwork();
    }
  }, [
    draftValues,
    pendingAdds,
    pendingDeletes,
    pendingDeleteKeys,
    junctionRowsAll,
    tankRowsAll,
    reservoirRowsAll,
    allElementIds,
    bumpNetwork,
    markEdited,
    showToast,
    project,
    activeScenarioId,
  ]);

  // Discard all staged changes/new rows and reset cells to server values.
  const handleDiscard = useCallback(() => {
    setPendingAdds([]);
    setPendingDeletes([]);
    setDraft(new Map());
    setDiscardGen((n) => n + 1);
  }, []);

  const shownRows =
    activeSection === "junctions"
      ? junctionRows.length
      : activeSection === "pipes"
        ? pipeRows.length
        : activeSection === "pumps"
          ? pumpRows.length
          : activeSection === "tanks"
            ? tankRows.length
            : activeSection === "valves"
              ? valveRows.length
              : reservoirRows.length;
  const totalRows =
    activeSection === "junctions"
      ? junctionRowsAllWithPending.length
      : activeSection === "pipes"
        ? pipeRowsAllWithPending.length
        : activeSection === "pumps"
          ? pumpRowsAllWithPending.length
          : activeSection === "tanks"
            ? tankRowsAllWithPending.length
            : activeSection === "valves"
              ? valveRowsAllWithPending.length
              : reservoirRowsAllWithPending.length;

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
        animation: "fadeIn 150ms ease-out",
      }}
    >
      {/* Section tab bar */}
      <div
        style={{
          height: 44,
          display: "flex",
          alignItems: "center",
          paddingLeft: 12,
          paddingRight: 12,
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-panel)",
          flexShrink: 0,
          gap: 4,
          minWidth: 0,
        }}
      >
        {/* Scrollable tab strip */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            overflowX: "auto",
            flexShrink: 1,
            minWidth: 0,
            scrollbarWidth: "none",
          }}
        >
          <TabButton
            variant="underline"
            active={activeSection === "junctions"}
            onClick={() => handleTabClick("junctions")}
            dirty={dirtyKinds.has("junction")}
          >{`Junctions (${junctionRowsAllWithPending.length})`}</TabButton>
          <TabButton
            variant="underline"
            active={activeSection === "pipes"}
            onClick={() => handleTabClick("pipes")}
            dirty={dirtyKinds.has("pipe")}
          >{`Pipes (${pipeRowsAllWithPending.length})`}</TabButton>
          <TabButton
            variant="underline"
            active={activeSection === "pumps"}
            onClick={() => handleTabClick("pumps")}
            dirty={dirtyKinds.has("pump")}
          >{`Pumps (${pumpRowsAllWithPending.length})`}</TabButton>
          <TabButton
            variant="underline"
            active={activeSection === "tanks"}
            onClick={() => handleTabClick("tanks")}
            dirty={dirtyKinds.has("tank")}
          >{`Tanks (${tankRowsAllWithPending.length})`}</TabButton>
          <TabButton
            variant="underline"
            active={activeSection === "reservoirs"}
            onClick={() => handleTabClick("reservoirs")}
            dirty={dirtyKinds.has("reservoir")}
          >{`Reservoirs (${reservoirRowsAllWithPending.length})`}</TabButton>
          <TabButton
            variant="underline"
            active={activeSection === "valves"}
            onClick={() => handleTabClick("valves")}
            dirty={dirtyKinds.has("valve")}
          >{`Valves (${valveRowsAllWithPending.length})`}</TabButton>
        </div>

        <div style={{ flex: 1 }} />

        {/* Search */}
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Search…"
          style={{
            width: 200,
            height: 28,
            background: "var(--bg-input)",
            border: "1px solid var(--border)",
            borderRadius: 5,
            padding: "0 8px",
            color: "var(--text-primary)",
            fontFamily: "var(--font-ui)",
            fontSize: 13,
            outline: "none",
          }}
        />

        {/* Add button */}
        <button
          type="button"
          onClick={handleAddElement}
          style={{
            background: "var(--accent-dim)",
            color: "var(--accent)",
            border: "1px solid var(--border-focus)",
            borderRadius: 5,
            padding: "0 10px",
            height: 28,
            fontSize: 12,
            fontFamily: "var(--font-ui)",
            cursor: "pointer",
            marginLeft: 6,
            whiteSpace: "nowrap",
          }}
        >
          + Add element
        </button>

        <button
          type="button"
          onClick={handleDeleteSelected}
          disabled={!selectedId}
          style={{
            background: "rgba(210, 80, 80, 0.12)",
            color: "rgba(240, 130, 130, 0.95)",
            border: "1px solid rgba(210, 80, 80, 0.35)",
            borderRadius: 5,
            padding: "0 10px",
            height: 28,
            fontSize: 12,
            fontFamily: "var(--font-ui)",
            cursor: selectedId ? "pointer" : "not-allowed",
            marginLeft: 6,
            whiteSpace: "nowrap",
            opacity: selectedId ? 1 : 0.45,
          }}
          title={
            selectedId
              ? "Stage delete for selected row"
              : "Select a row to delete"
          }
        >
          Delete selected
        </button>
      </div>

      {/* Table */}
      <div
        ref={tableScrollRef}
        style={{ flex: 1, overflow: "auto", minHeight: 0 }}
      >
        {activeSection === "junctions" && (
          <JunctionTable
            rows={junctionRows}
            sortField={sortField}
            sortAsc={sortAsc}
            selectedId={selectedId}
            onSort={handleSort}
            onSelect={setSelectedId}
            onPatch={handleStage}
            pendingKeys={pendingKeys}
            pendingRowIds={pendingRowIds}
            discardGen={discardGen}
            scrollContainerRef={tableScrollRef}
          />
        )}
        {activeSection === "pipes" && (
          <PipeTable
            rows={pipeRows}
            sortField={sortField}
            sortAsc={sortAsc}
            selectedId={selectedId}
            onSort={handleSort}
            onSelect={setSelectedId}
            onPatch={handleStage}
            nodeOptions={nodeReferenceOptions}
            pendingKeys={pendingKeys}
            pendingRowIds={pendingRowIds}
            discardGen={discardGen}
            scrollContainerRef={tableScrollRef}
          />
        )}
        {activeSection === "pumps" && (
          <PumpTable
            rows={pumpRows}
            sortField={sortField}
            sortAsc={sortAsc}
            selectedId={selectedId}
            onSort={handleSort}
            onSelect={setSelectedId}
            onPatch={handleStage}
            nodeOptions={nodeReferenceOptions}
            pendingKeys={pendingKeys}
            pendingRowIds={pendingRowIds}
            discardGen={discardGen}
            scrollContainerRef={tableScrollRef}
            focusToken={focusPumpToken}
          />
        )}
        {activeSection === "tanks" && (
          <TankTable
            rows={tankRows}
            sortField={sortField}
            sortAsc={sortAsc}
            selectedId={selectedId}
            onSort={handleSort}
            onSelect={setSelectedId}
            onPatch={handleStage}
            pendingKeys={pendingKeys}
            pendingRowIds={pendingRowIds}
            discardGen={discardGen}
            scrollContainerRef={tableScrollRef}
          />
        )}
        {activeSection === "reservoirs" && (
          <ReservoirTable
            rows={reservoirRows}
            sortField={sortField}
            sortAsc={sortAsc}
            selectedId={selectedId}
            onSort={handleSort}
            onSelect={setSelectedId}
            onPatch={handleStage}
            pendingKeys={pendingKeys}
            pendingRowIds={pendingRowIds}
            discardGen={discardGen}
            scrollContainerRef={tableScrollRef}
          />
        )}
        {activeSection === "valves" && (
          <ValveTable
            rows={valveRows}
            sortField={sortField}
            sortAsc={sortAsc}
            selectedId={selectedId}
            onSort={handleSort}
            onSelect={setSelectedId}
            onPatch={handleStage}
            nodeOptions={nodeReferenceOptions}
            pendingKeys={pendingKeys}
            pendingRowIds={pendingRowIds}
            discardGen={discardGen}
            scrollContainerRef={tableScrollRef}
          />
        )}
      </div>

      {/* Status / save bar */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "6px 16px",
          borderTop: `1px solid ${dirtyCount > 0 ? "rgba(220, 160, 40, 0.3)" : "var(--border)"}`,
          flexShrink: 0,
          fontSize: 12,
          background: dirtyCount > 0 ? "rgba(220, 160, 40, 0.07)" : undefined,
          transition: "background 200ms",
        }}
      >
        {dirtyCount > 0 ? (
          <>
            <span style={{ color: "rgba(220, 160, 40, 0.9)", fontWeight: 500 }}>
              {dirtyCount} unsaved change{dirtyCount !== 1 ? "s" : ""}
            </span>
            <div style={{ flex: 1 }} />
            {dirtyCount > 0 && (
              <button
                type="button"
                onClick={() => setPreviewOpen(true)}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLButtonElement).style.background =
                    "var(--nav-hover)";
                  (e.currentTarget as HTMLButtonElement).style.borderColor =
                    "var(--border-hover)";
                  (e.currentTarget as HTMLButtonElement).style.color =
                    "var(--text-primary)";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLButtonElement).style.background =
                    "transparent";
                  (e.currentTarget as HTMLButtonElement).style.borderColor =
                    "var(--border)";
                  (e.currentTarget as HTMLButtonElement).style.color =
                    "var(--text-secondary)";
                }}
                style={{
                  padding: "4px 12px",
                  borderRadius: 5,
                  border: "1px solid var(--border)",
                  background: "transparent",
                  color: "var(--text-secondary)",
                  fontFamily: "var(--font-ui)",
                  fontSize: 12,
                  cursor: "pointer",
                  transition:
                    "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
                }}
              >
                Preview changes
              </button>
            )}
            <button
              type="button"
              onClick={handleDiscard}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "var(--nav-hover)";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "var(--border-hover)";
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-primary)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "transparent";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "var(--border)";
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-secondary)";
              }}
              style={{
                padding: "4px 12px",
                borderRadius: 5,
                border: "1px solid var(--border)",
                background: "transparent",
                color: "var(--text-secondary)",
                fontFamily: "var(--font-ui)",
                fontSize: 12,
                cursor: "pointer",
                transition:
                  "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
              }}
            >
              Discard
            </button>
            <button
              type="button"
              onClick={handleSave}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "rgba(220, 160, 40, 0.22)";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "rgba(220, 160, 40, 0.65)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "rgba(220, 160, 40, 0.12)";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "rgba(220, 160, 40, 0.4)";
              }}
              style={{
                padding: "4px 12px",
                borderRadius: 5,
                border: "1px solid rgba(220, 160, 40, 0.4)",
                background: "rgba(220, 160, 40, 0.12)",
                color: "rgba(220, 160, 40, 0.95)",
                fontFamily: "var(--font-ui)",
                fontSize: 12,
                fontWeight: 500,
                cursor: "pointer",
                transition:
                  "background var(--t-fast), border-color var(--t-fast)",
              }}
            >
              Save changes
            </button>
          </>
        ) : (
          <span style={{ color: "var(--text-tertiary)" }}>
            Showing {shownRows} of {totalRows} elements
          </span>
        )}
      </div>

      {/* Preview changes modal */}
      {previewOpen && project && (
        <InpDiffModal
          patches={previewPatches}
          onClose={() => setPreviewOpen(false)}
        />
      )}
    </div>
  );
}
