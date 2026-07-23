import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../../AppContext";
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
import { ELEMENT_TEMP_ID_PREFIX, useDraft } from "../../../hooks/DraftContext";
import {
  collectDirtyKinds,
  type ElementKind,
} from "./elementsEditorDerivations";
import { JunctionTable } from "./JunctionTable";
import { PipeTable } from "./PipeTable";
import { PumpTable } from "./PumpTable";
import { ReservoirTable } from "./ReservoirTable";
import { TankTable } from "./TankTable";
import { compareIds, filterSortRows, SEARCH_DEBOUNCE_MS } from "./tableSearch";
import { ValveTable } from "./ValveTable";

type Section =
  | "junctions"
  | "pipes"
  | "pumps"
  | "tanks"
  | "reservoirs"
  | "valves";

const TEMP_ID_PREFIX = ELEMENT_TEMP_ID_PREFIX;

export function ElementsEditor({
  focusPumpId,
  focusPumpToken,
}: {
  /** Pump ID to select when `focusPumpToken` changes (e.g. "attached to" link
   *  clicked from the Pump curves tab). */
  focusPumpId?: string;
  /** Bump this (e.g. `Date.now()`) to re-trigger the jump even for the same id. */
  focusPumpToken?: number;
}) {
  const {
    elementsDraft: draft,
    setElementsDraft: setDraft,
    pendingAdds,
    setPendingAdds,
    pendingDeletes,
    setPendingDeletes,
    nextTempIndex,
  } = useDraft();
  const junctionRowsAll = useJunctionRows();
  const pipeRowsAll = usePipeRows();
  const pumpRowsAll = usePumpRows();
  const tankRowsAll = useTankRows();
  const reservoirRowsAll = useReservoirRows();
  const valveRowsAll = useValveRows();
  // The editor stays mounted (display:none) while other project views are
  // active so drafts survive tab switches — skip rebuilding the filtered +
  // sorted row models while it is hidden.
  const { projectView } = useAppState();
  const editorVisible = projectView === "editor";
  const [activeSection, setActiveSection] = useState<Section>("junctions");
  const [searchQuery, setSearchQuery] = useState("");
  // Filtering runs against a debounced copy of the query so fast typing does
  // not re-filter ~46k rows on every keystroke. Clearing is applied
  // immediately (tab switches and the clear action should not lag).
  const [debouncedQuery, setDebouncedQuery] = useState("");
  useEffect(() => {
    if (searchQuery === "") {
      setDebouncedQuery("");
      return;
    }
    const t = window.setTimeout(
      () => setDebouncedQuery(searchQuery),
      SEARCH_DEBOUNCE_MS,
    );
    return () => window.clearTimeout(t);
  }, [searchQuery]);
  // `null` = no explicit sort: rows keep network order and filterSort skips
  // the O(N log N) copy + sort entirely.
  const [sortField, setSortField] = useState<string | null>(null);
  const [sortAsc, setSortAsc] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [discardGen, setDiscardGen] = useState(0);
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

  // Reset EditableCell drafts to committed values whenever the elements
  // draft becomes empty (a save or discard) — this is what previously
  // happened inside this component's own handleSave/handleDiscard, now
  // triggered from the DraftContext (which may be cleared from the global
  // save bar in NetworkEditor.tsx).
  const elementsDirtyCount =
    draft.size + pendingAdds.length + pendingDeletes.length;
  const prevElementsDirtyCount = useRef(elementsDirtyCount);
  useEffect(() => {
    if (prevElementsDirtyCount.current > 0 && elementsDirtyCount === 0) {
      setDiscardGen((n) => n + 1);
    }
    prevElementsDirtyCount.current = elementsDirtyCount;
  }, [elementsDirtyCount]);

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

  // Stage a change locally without writing to the backend yet.
  const handleStage = useCallback(
    (kind: string, id: string, field: string, value: number | string) => {
      setDraft((prev) => {
        const next = new Map(prev);
        next.set(`${kind}:${id}:${field}`, { kind, id, field, value });
        return next;
      });
    },
    [setDraft],
  );

  const handleTabClick = (section: Section) => {
    setActiveSection(section);
    setSearchQuery("");
    setSortField(null);
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

  // Search + sort only ever apply to the visible section: the query and sort
  // state are reset on tab switch, so passing an empty query / null sort for
  // the five hidden sections is behaviour-preserving while making each
  // keystroke cost one section's filter instead of six (~650k stringify +
  // lowercase calls per keystroke at 46k-node scale before).
  const junctionsActive = activeSection === "junctions";
  const pipesActive = activeSection === "pipes";
  const pumpsActive = activeSection === "pumps";
  const tanksActive = activeSection === "tanks";
  const reservoirsActive = activeSection === "reservoirs";
  const valvesActive = activeSection === "valves";

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

  // While the editor tab is hidden the query/sort row models are not shown
  // anywhere, so fall back to the raw arrays and recompute on next reveal.
  // Per-section query/sort inputs: "" / null for every inactive section, so
  // a keystroke only invalidates the active section's memo.
  const junctionQuery = junctionsActive ? debouncedQuery : "";
  const junctionSortField = junctionsActive ? sortField : null;
  const junctionRows = useMemo(
    () =>
      editorVisible
        ? filterSortRows(
            junctionRowsAllWithPending,
            junctionQuery,
            junctionSortField,
            sortAsc,
          )
        : junctionRowsAllWithPending,
    [
      editorVisible,
      junctionRowsAllWithPending,
      junctionQuery,
      junctionSortField,
      sortAsc,
    ],
  );
  const pipeQuery = pipesActive ? debouncedQuery : "";
  const pipeSortField = pipesActive ? sortField : null;
  const pipeRows = useMemo(
    () =>
      editorVisible
        ? filterSortRows(
            pipeRowsAllWithPending,
            pipeQuery,
            pipeSortField,
            sortAsc,
          )
        : pipeRowsAllWithPending,
    [editorVisible, pipeRowsAllWithPending, pipeQuery, pipeSortField, sortAsc],
  );
  const pumpQuery = pumpsActive ? debouncedQuery : "";
  const pumpSortField = pumpsActive ? sortField : null;
  const pumpRows = useMemo(
    () =>
      editorVisible
        ? filterSortRows(
            pumpRowsAllWithPending,
            pumpQuery,
            pumpSortField,
            sortAsc,
          )
        : pumpRowsAllWithPending,
    [editorVisible, pumpRowsAllWithPending, pumpQuery, pumpSortField, sortAsc],
  );
  const tankQuery = tanksActive ? debouncedQuery : "";
  const tankSortField = tanksActive ? sortField : null;
  const tankRows = useMemo(
    () =>
      editorVisible
        ? filterSortRows(
            tankRowsAllWithPending,
            tankQuery,
            tankSortField,
            sortAsc,
          )
        : tankRowsAllWithPending,
    [editorVisible, tankRowsAllWithPending, tankQuery, tankSortField, sortAsc],
  );
  const reservoirQuery = reservoirsActive ? debouncedQuery : "";
  const reservoirSortField = reservoirsActive ? sortField : null;
  const reservoirRows = useMemo(
    () =>
      editorVisible
        ? filterSortRows(
            reservoirRowsAllWithPending,
            reservoirQuery,
            reservoirSortField,
            sortAsc,
          )
        : reservoirRowsAllWithPending,
    [
      editorVisible,
      reservoirRowsAllWithPending,
      reservoirQuery,
      reservoirSortField,
      sortAsc,
    ],
  );
  const valveQuery = valvesActive ? debouncedQuery : "";
  const valveSortField = valvesActive ? sortField : null;
  const valveRows = useMemo(
    () =>
      editorVisible
        ? filterSortRows(
            valveRowsAllWithPending,
            valveQuery,
            valveSortField,
            sortAsc,
          )
        : valveRowsAllWithPending,
    [
      editorVisible,
      valveRowsAllWithPending,
      valveQuery,
      valveSortField,
      sortAsc,
    ],
  );

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

    // Shared-collator sort: per-comparison localeCompare re-resolves locale
    // data and is measurably slower over ~46k ids.
    return Array.from(ids).sort(compareIds);
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
    setSortField(null);
    setSortAsc(true);
    setSelectedId(tempId);
  }, [activeKind, nextTempIndex, setPendingAdds]);

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
  }, [activeKind, selectedId, setPendingAdds, setPendingDeletes, setDraft]);

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
            sortField={sortField ?? ""}
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
            sortField={sortField ?? ""}
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
            sortField={sortField ?? ""}
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
            sortField={sortField ?? ""}
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
            sortField={sortField ?? ""}
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
            sortField={sortField ?? ""}
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

      {/* Status bar — Save/Discard/Preview now live in the unified bar at
          the bottom of NetworkEditor.tsx, spanning all four tabs. */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "6px 16px",
          borderTop: "1px solid var(--border)",
          flexShrink: 0,
          fontSize: 12,
        }}
      >
        <span style={{ color: "var(--text-tertiary)" }}>
          Showing {shownRows} of {totalRows} elements
        </span>
      </div>
    </div>
  );
}
