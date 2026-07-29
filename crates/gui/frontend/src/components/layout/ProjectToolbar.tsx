import {
  ArrowRightIcon,
  ChevronDownIcon,
  Cog6ToothIcon,
  MagnifyingGlassIcon,
  PlayIcon,
} from "@heroicons/react/16/solid";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { type ScenarioDto, useScenarios } from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import { formatPrimaryShortcut } from "../../shortcuts";
import {
  activeLineage,
  type FlatScenario,
  flattenScenarios,
  flattenSubtrees,
  lineageLabel,
  scenarioChildren,
} from "../panels/ScenariosPanel/shared";
import { PrimaryButton } from "../ui/PrimaryButton";

/* ─── ProjectToolbar ────────────────────────────────────────────────────────
   Persistent toolbar across the top of every project view. Holds scenario
   selection (a "Base" pill always present and selected by default when
   activeScenarioId === null) plus the run controls.

   The strip is a LINEAGE view, not the whole tree: it renders only the
   active path Base → … → active scenario, so every arrow connector is a true
   parent→child edge. Branching is reachable in place: chips with siblings
   grow a ▾ "+N" segment opening a sibling picker, and when the active
   scenario (or Base) has children a trailing ▾ stub lets the user descend.
   Everything else lives in the Manage modal.

   Layout (left → right):
     [Base pill] → [lineage chip → …] → [▾ children stub] · Manage | [Simulate ▸ ⚙]
*/

// ── Types ─────────────────────────────────────────────────────────────────────

type ScenarioState =
  | "not-run"
  | "draft"
  | "ready"
  | "running"
  | "simulated"
  | "calibrated"
  | "failed";

/** One open strip dropdown: what it lists + its fixed-position anchor
 * (fixed positioning escapes the strip's overflow clipping). */
interface PickerAnchor {
  /** "siblings"/"children" show an indented mini-subtree of that branch
   * point; "search" is the quick-jump list over ALL scenarios. */
  kind: "siblings" | "children" | "search";
  /** Sibling picker: the chip's scenario id. Children picker: the active
   * scenario id (`null` = Base's direct children). Search: unused (null). */
  id: string | null;
  left: number;
  top: number;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const STATE_COLOR: Record<ScenarioState, string> = {
  "not-run": "#6b7480",
  draft: "#6b7480",
  ready: "#6b7480",
  running: "#d9aa57",
  simulated: "#7bbf95",
  calibrated: "#7aa3d9",
  failed: "#d97b7b",
};

const STATE_LABEL: Record<ScenarioState, string> = {
  "not-run": "not run",
  draft: "not run",
  ready: "not run",
  running: "running…",
  simulated: "simulated",
  calibrated: "calibrated",
  failed: "failed",
};

const STALE_COLOR = "#f59e0b";

function stateColor(state: string, isStale: boolean): string {
  return isStale
    ? STALE_COLOR
    : (STATE_COLOR[state as ScenarioState] ?? STATE_COLOR["not-run"]);
}

function stateLabel(state: string, isStale: boolean): string {
  if (isStale) return "edited";
  return STATE_LABEL[state as ScenarioState] ?? state;
}

/** Parent→child connector between strip entries. */
function LineageArrow() {
  return (
    <span
      aria-hidden
      style={{
        color: "var(--text-disabled)",
        flexShrink: 0,
        display: "inline-flex",
        alignItems: "center",
      }}
    >
      <ArrowRightIcon style={{ width: 12, height: 12 }} />
    </span>
  );
}

// ── Component ─────────────────────────────────────────────────────────────────

export function ProjectToolbar() {
  const {
    openRunModal,
    openSimSettingsModal,
    openScenariosModal,
    activeScenarioId,
    setActiveScenarioId,
    scenariosVersion,
  } = useAppState();
  const { project, accent } = useActiveProject();
  const { isEdited, markEdited } = useNetworkVersion();

  const rawDtos = useScenarios(project?.id ?? null, scenariosVersion);

  // Active path Base → … → active. Empty when Base is active or the stored
  // id is stale/unknown (the reset effect below then falls back to Base).
  const lineage = useMemo(
    () => activeLineage(rawDtos, activeScenarioId),
    [rawDtos, activeScenarioId],
  );
  // Children of the active scenario (or of Base when none is active) — they
  // drive the trailing ▾ stub that lets the user walk DOWN a branch.
  const activeChildren = useMemo(
    () =>
      scenarioChildren(
        rawDtos,
        lineage.length > 0 ? (activeScenarioId ?? null) : null,
      ),
    [rawDtos, lineage, activeScenarioId],
  );

  // One picker open at a time; anchored via fixed coordinates captured from
  // the trigger so the dropdown escapes the strip's overflow clipping.
  const [picker, setPicker] = useState<PickerAnchor | null>(null);
  // Quick-jump filter text (search picker only).
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Autofocus the quick-jump input when its picker opens.
  useEffect(() => {
    if (picker?.kind === "search") {
      setTimeout(() => searchInputRef.current?.focus(), 0);
    }
  }, [picker?.kind]);

  // Click-outside (CanvasToolbar convention, scoped by data attribute since
  // this toolbar has no global dropdown owner).
  useEffect(() => {
    if (!picker) return;
    const onPointerDown = (e: PointerEvent) => {
      const el = e.target instanceof Element ? e.target : null;
      if (el?.closest("[data-scenario-picker]")) return;
      setPicker(null);
    };
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [picker]);

  // If the active scenario was deleted, fall back to Base.
  // Guard on rawDtos.length > 0 so we don't reset before the list loads.
  useEffect(() => {
    if (
      activeScenarioId &&
      rawDtos.length > 0 &&
      !rawDtos.find((s) => s.id === activeScenarioId)
    ) {
      setActiveScenarioId(null);
    }
  }, [rawDtos, activeScenarioId, setActiveScenarioId]);

  // Seed the edited set from DB-persisted stale state so the amber
  // indicators survive app restarts. markEdited is idempotent.
  useEffect(() => {
    if (!project?.id) return;
    for (const s of rawDtos) {
      if (s.state === "stale") markEdited(project.id, s.id);
    }
  }, [rawDtos, markEdited, project?.id]);

  useEffect(() => {
    if (project?.state === "stale") markEdited(project.id, null);
  }, [project?.state, project?.id, markEdited]);

  if (!project) return null;

  const activeScenario = activeScenarioId
    ? (rawDtos.find((s) => s.id === activeScenarioId) ?? null)
    : null;

  // Rows listed by the open sibling/children picker: an indented
  // mini-subtree of the branch point (each sibling/child with all its
  // descendants), resolved live so renames/deletes while open stay correct.
  const pickerOptions: FlatScenario[] = (() => {
    if (!picker || picker.kind === "search") return [];
    if (picker.kind === "children") {
      return flattenSubtrees(
        rawDtos,
        scenarioChildren(rawDtos, picker.id).map((s) => s.id),
      );
    }
    const chip = rawDtos.find((d) => d.id === picker.id);
    if (!chip) return [];
    return flattenSubtrees(
      rawDtos,
      scenarioChildren(rawDtos, chip.parentScenarioId ?? null)
        .filter((s) => s.id !== chip.id)
        .map((s) => s.id),
    );
  })();

  // Quick-jump rows: every scenario in tree order, filtered as you type.
  const searchOptions: ScenarioDto[] = (() => {
    if (picker?.kind !== "search") return [];
    const q = searchQuery.trim().toLowerCase();
    const all = flattenScenarios(rawDtos);
    return q ? all.filter((s) => s.name.toLowerCase().includes(q)) : all;
  })();

  const baseActive = activeScenarioId === null;
  const baseStale = isEdited(project.id, null) || project.state === "stale";
  const baseState = (project.state ?? "draft") as ScenarioState;
  const baseEffectiveColor = baseStale
    ? STALE_COLOR
    : (STATE_COLOR[baseState] ?? STATE_COLOR["not-run"]);
  const baseRunning = baseState === "running";
  const baseTitle = `Base model · ${STATE_LABEL[baseState] ?? baseState}${baseStale ? " · network edited since last run" : ""}`;

  // Active-scenario-scoped flags that drive the Run button appearance.
  const activeIsStale =
    activeScenarioId === null
      ? baseStale
      : isEdited(project.id, activeScenarioId) ||
        activeScenario?.state === "stale";
  const activeIsSimulated =
    !activeIsStale &&
    (activeScenarioId === null
      ? baseState === "simulated"
      : activeScenario?.state === "simulated");
  const runBtnClass = activeIsStale
    ? "btn-run--stale"
    : activeIsSimulated
      ? "btn-run--outline"
      : undefined;
  const runBtnLabel = "Simulate";
  const runShortcut = formatPrimaryShortcut("R");
  const runBtnTitle = activeIsStale
    ? "Network edited since last run. Rerun simulation."
    : `Run simulation (${runShortcut})`;

  const togglePicker = (
    next: Omit<PickerAnchor, "left" | "top">,
    rect: DOMRect,
  ) => {
    setSearchQuery("");
    setPicker((prev) =>
      prev && prev.kind === next.kind && prev.id === next.id
        ? null
        : { ...next, left: rect.left, top: rect.bottom + 4 },
    );
  };

  const selectScenario = (id: string) => {
    setActiveScenarioId(id);
    setPicker(null);
  };

  return (
    <div
      style={{
        flexShrink: 0,
        height: 44,
        background: "var(--bg-panel)",
        borderBottom: "1px solid var(--border)",
        display: "flex",
        alignItems: "center",
        gap: 10,
        paddingLeft: 14,
        paddingRight: 14,
        overflow: "hidden",
        fontFamily: "var(--font-ui)",
      }}
    >
      {/* Section label — names the whole selector (Base model + scenarios) */}
      <span
        style={{
          fontSize: 10,
          fontWeight: 600,
          letterSpacing: "0.07em",
          textTransform: "uppercase",
          color: "var(--text-disabled)",
          flexShrink: 0,
          userSelect: "none",
        }}
      >
        Scenarios
      </span>

      {/* Base pill — the base model itself; always present and rendered bold so
          it reads as *the* base model, not a scenario named "Base". Active when
          no scenario is selected. */}
      <button
        type="button"
        onClick={() => setActiveScenarioId(null)}
        data-tooltip={baseTitle}
        data-tooltip-pos="bottom"
        onMouseEnter={(e) => {
          if (!baseActive) {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--nav-hover)";
            (e.currentTarget as HTMLButtonElement).style.borderColor =
              "var(--border-hover)";
          }
        }}
        onMouseLeave={(e) => {
          if (!baseActive) {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--bg-card)";
            (e.currentTarget as HTMLButtonElement).style.borderColor =
              "var(--border)";
          }
        }}
        style={{
          flexShrink: 0,
          display: "inline-flex",
          alignItems: "center",
          gap: 5,
          padding: "4px 10px 4px 8px",
          border: baseActive
            ? `1px solid ${accent}`
            : "1px solid var(--border)",
          borderRadius: 14,
          background: baseActive ? `${accent}22` : "var(--bg-card)",
          color: baseActive ? accent : "var(--text-secondary)",
          fontSize: 11,
          fontWeight: 700,
          cursor: "pointer",
          fontFamily: "var(--font-ui)",
          transition: "background var(--t-fast), border-color var(--t-fast)",
        }}
      >
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: baseEffectiveColor,
            flexShrink: 0,
            boxShadow:
              baseRunning || baseStale
                ? `0 0 6px ${baseEffectiveColor}`
                : "none",
            animation: baseRunning
              ? "pulseDot 1.4s ease-in-out infinite"
              : "none",
          }}
        />
        Base
      </button>

      {/* Scrollable lineage strip with right-edge fade */}
      {(lineage.length > 0 || activeChildren.length > 0) && (
        <div
          style={{
            flex: 1,
            position: "relative",
            minWidth: 0,
            overflow: "hidden",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              overflowX: "auto",
              overflowY: "hidden",
              scrollbarWidth: "none",
              paddingRight: 24,
            }}
          >
            {lineage.map((s) => {
              const siblingCount = scenarioChildren(
                rawDtos,
                s.parentScenarioId ?? null,
              ).filter((x) => x.id !== s.id).length;
              return (
                <Fragment key={s.id}>
                  <LineageArrow />
                  <ScenarioChip
                    scenario={s}
                    isActive={s.id === activeScenarioId}
                    isStale={isEdited(project.id, s.id) || s.state === "stale"}
                    accent={accent}
                    siblingCount={siblingCount}
                    pickerOpen={
                      picker?.kind === "siblings" && picker.id === s.id
                    }
                    onClick={() => setActiveScenarioId(s.id)}
                    onTogglePicker={(rect) =>
                      togglePicker({ kind: "siblings", id: s.id }, rect)
                    }
                  />
                </Fragment>
              );
            })}

            {/* Descend stub — children of the active scenario (or of Base).
                Without it, walking DOWN a branch you just left would require
                the Manage modal. */}
            {activeChildren.length > 0 && (
              <>
                <LineageArrow />
                <button
                  type="button"
                  data-scenario-picker
                  data-tooltip="Descend to a child scenario"
                  data-tooltip-pos="bottom"
                  onClick={(e) =>
                    togglePicker(
                      { kind: "children", id: activeScenarioId },
                      e.currentTarget.getBoundingClientRect(),
                    )
                  }
                  style={{
                    flexShrink: 0,
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    padding: "4px 9px",
                    border: "1px dashed var(--border-hover)",
                    borderRadius: 14,
                    background:
                      picker?.kind === "children"
                        ? "var(--nav-hover)"
                        : "transparent",
                    color: "var(--text-tertiary)",
                    fontSize: 11,
                    fontWeight: 500,
                    cursor: "pointer",
                    fontFamily: "var(--font-ui)",
                    whiteSpace: "nowrap",
                    transition: "background var(--t-fast)",
                  }}
                >
                  <ChevronDownIcon style={{ width: 10, height: 10 }} />
                  {activeChildren.length}{" "}
                  {activeChildren.length === 1 ? "child" : "children"}
                </button>
              </>
            )}
          </div>
          {/* Right fade overlay */}
          <div
            style={{
              position: "absolute",
              top: 0,
              right: 0,
              width: 24,
              height: "100%",
              background:
                "linear-gradient(to right, transparent, var(--bg-panel))",
              pointerEvents: "none",
            }}
          />
        </div>
      )}

      {/* Sibling / children picker — an indented mini-subtree of the branch
          point. Fixed-position so the strip's overflow clipping can't cut it
          off; internally scrolled for huge subtrees. */}
      {picker && picker.kind !== "search" && pickerOptions.length > 0 && (
        <div
          data-scenario-picker
          style={{
            position: "fixed",
            top: picker.top,
            left: picker.left,
            background: "var(--bg-panel)",
            border: "1px solid var(--border)",
            borderRadius: 7,
            boxShadow: "var(--shadow-2)",
            overflow: "hidden auto",
            minWidth: 180,
            maxWidth: 280,
            maxHeight: 260,
            zIndex: 120,
          }}
        >
          {pickerOptions.map((s) => (
            <PickerRow
              key={s.id}
              scenario={s}
              depth={s.depth}
              isStale={isEdited(project.id, s.id) || s.state === "stale"}
              onSelect={() => selectScenario(s.id)}
            />
          ))}
        </div>
      )}

      {/* Quick-jump picker — filter-as-you-type over ALL scenarios, with
          lineage breadcrumbs. Enter switches to the first match. */}
      {picker?.kind === "search" && (
        <div
          data-scenario-picker
          style={{
            position: "fixed",
            top: picker.top,
            left: picker.left,
            background: "var(--bg-panel)",
            border: "1px solid var(--border)",
            borderRadius: 7,
            boxShadow: "var(--shadow-2)",
            overflow: "hidden",
            width: 260,
            zIndex: 120,
            display: "flex",
            flexDirection: "column",
          }}
        >
          <input
            ref={searchInputRef}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setPicker(null);
              if (e.key === "Enter" && searchOptions.length > 0) {
                selectScenario(searchOptions[0].id);
              }
            }}
            placeholder="Jump to scenario…"
            style={{
              margin: 8,
              padding: "5px 8px",
              fontSize: 12,
              background: "var(--bg-input, var(--bg-card))",
              border: "1px solid var(--border)",
              borderRadius: 5,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
              outline: "none",
            }}
          />
          <div style={{ overflowY: "auto", maxHeight: 260 }}>
            {searchOptions.length === 0 && (
              <div
                style={{
                  padding: "10px 12px",
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                  fontFamily: "var(--font-ui)",
                }}
              >
                No scenarios match &ldquo;{searchQuery}&rdquo;
              </div>
            )}
            {searchOptions.map((s) => (
              <PickerRow
                key={s.id}
                scenario={s}
                subtitle={lineageLabel(rawDtos, s.id)}
                isStale={isEdited(project.id, s.id) || s.state === "stale"}
                isActive={s.id === activeScenarioId}
                onSelect={() => selectScenario(s.id)}
              />
            ))}
          </div>
        </div>
      )}

      {/* Quick-jump — searchable list of ALL scenarios */}
      {rawDtos.length > 0 && (
        <button
          type="button"
          data-scenario-picker
          aria-label="Find scenario"
          data-tooltip="Find scenario"
          data-tooltip-pos="bottom"
          onClick={(e) =>
            togglePicker(
              { kind: "search", id: null },
              e.currentTarget.getBoundingClientRect(),
            )
          }
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
              picker?.kind === "search" ? "var(--nav-hover)" : "transparent";
            (e.currentTarget as HTMLButtonElement).style.borderColor =
              "var(--border)";
            (e.currentTarget as HTMLButtonElement).style.color =
              "var(--text-secondary)";
          }}
          style={{
            flexShrink: 0,
            width: 26,
            height: 26,
            border: "1px solid var(--border)",
            background:
              picker?.kind === "search" ? "var(--nav-hover)" : "transparent",
            color: "var(--text-secondary)",
            borderRadius: 5,
            padding: 0,
            cursor: "pointer",
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            transition:
              "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
          }}
        >
          <MagnifyingGlassIcon style={{ width: 12, height: 12 }} />
        </button>
      )}

      {/* Manage — opens the Scenarios management modal */}
      <button
        type="button"
        onClick={() => openScenariosModal()}
        data-tooltip="Manage scenarios"
        data-tooltip-pos="bottom"
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
          flexShrink: 0,
          height: 26,
          border: "1px solid var(--border)",
          background: "transparent",
          color: "var(--text-secondary)",
          borderRadius: 5,
          padding: "0 8px",
          fontSize: 11,
          fontWeight: 600,
          cursor: "pointer",
          fontFamily: "var(--font-ui)",
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          transition:
            "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
        }}
      >
        <Cog6ToothIcon style={{ width: 12, height: 12 }} />
        Manage
      </button>

      {/* Divider separating scenario controls from the run controls.
          marginLeft: auto pushes this + the split button to the far right. */}
      <span
        style={{
          width: 1,
          height: 22,
          background: "var(--border)",
          flexShrink: 0,
          marginLeft: "auto",
        }}
      />

      {/* Split Simulate button — left segment runs; right (gear) segment opens
          the simulation-settings modal. */}
      <div
        style={{
          flexShrink: 0,
          display: "inline-flex",
          alignItems: "stretch",
        }}
      >
        <PrimaryButton
          size="sm"
          onClick={openRunModal}
          className={runBtnClass}
          data-tooltip={runBtnTitle}
          data-tooltip-pos="bottom"
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 5,
            borderTopRightRadius: 0,
            borderBottomRightRadius: 0,
          }}
        >
          <PlayIcon style={{ width: 12, height: 12 }} />
          {runBtnLabel}
        </PrimaryButton>
        <PrimaryButton
          size="sm"
          onClick={openSimSettingsModal}
          className={runBtnClass}
          aria-label="Simulation settings"
          data-tooltip="Simulation settings"
          data-tooltip-pos="bottom"
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "0 8px",
            borderTopLeftRadius: 0,
            borderBottomLeftRadius: 0,
            borderLeft: "1px solid rgba(255,255,255,0.28)",
          }}
        >
          <Cog6ToothIcon style={{ width: 13, height: 13 }} />
        </PrimaryButton>
      </div>
    </div>
  );
}

// ── ScenarioChip ──────────────────────────────────────────────────────────────

/** One lineage chip: the scenario itself (click = activate) plus, when it
 * has siblings, an attached ▾ "+N" segment opening the sibling picker. */
function ScenarioChip({
  scenario,
  isActive,
  isStale,
  accent,
  siblingCount,
  pickerOpen,
  onClick,
  onTogglePicker,
}: {
  scenario: ScenarioDto;
  isActive: boolean;
  isStale: boolean;
  accent: string;
  siblingCount: number;
  pickerOpen: boolean;
  onClick: () => void;
  onTogglePicker: (anchor: DOMRect) => void;
}) {
  const state = (scenario.state ?? "not-run") as ScenarioState;
  const effectiveColor = isStale
    ? STALE_COLOR
    : (STATE_COLOR[state] ?? STATE_COLOR["not-run"]);
  const isRunning = state === "running";
  const titleSuffix = isStale ? " · network edited since last run" : "";
  const textColor = isActive ? accent : "var(--text-primary)";

  return (
    <span
      style={{
        flexShrink: 0,
        display: "inline-flex",
        alignItems: "stretch",
        border: isActive ? `1px solid ${accent}` : "1px solid var(--border)",
        borderRadius: 14,
        background: isActive ? `${accent}22` : "var(--bg-card)",
        overflow: "hidden",
        transition: "background var(--t-fast), border-color var(--t-fast)",
      }}
    >
      <button
        type="button"
        onClick={onClick}
        data-tooltip={`${scenario.name} · ${STATE_LABEL[state] ?? state}${titleSuffix}`}
        data-tooltip-pos="bottom"
        onMouseEnter={(e) => {
          if (!isActive) {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--nav-hover)";
          }
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "transparent";
        }}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 5,
          padding: "4px 9px 4px 7px",
          border: "none",
          background: "transparent",
          color: textColor,
          fontSize: 11,
          fontWeight: isActive ? 700 : 500,
          cursor: "pointer",
          fontFamily: "var(--font-ui)",
          transition: "background var(--t-fast)",
          whiteSpace: "nowrap",
        }}
      >
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: effectiveColor,
            flexShrink: 0,
            boxShadow:
              isRunning || isStale ? `0 0 6px ${effectiveColor}` : "none",
            animation: isRunning
              ? "pulseDot 1.4s ease-in-out infinite"
              : "none",
          }}
        />
        {scenario.name}
      </button>

      {siblingCount > 0 && (
        <button
          type="button"
          data-scenario-picker
          aria-label={`Switch to a sibling of ${scenario.name}`}
          data-tooltip={`${siblingCount} sibling ${siblingCount === 1 ? "scenario" : "scenarios"}`}
          data-tooltip-pos="bottom"
          onClick={(e) => {
            e.stopPropagation();
            onTogglePicker(e.currentTarget.getBoundingClientRect());
          }}
          onMouseEnter={(e) => {
            if (!pickerOpen) {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--nav-hover)";
            }
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background = pickerOpen
              ? "var(--nav-hover)"
              : "transparent";
          }}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 2,
            padding: "0 7px 0 5px",
            border: "none",
            borderLeft: isActive
              ? `1px solid ${accent}55`
              : "1px solid var(--border)",
            background: pickerOpen ? "var(--nav-hover)" : "transparent",
            color: isActive ? accent : "var(--text-tertiary)",
            fontSize: 10,
            fontWeight: 600,
            cursor: "pointer",
            fontFamily: "var(--font-ui)",
            transition: "background var(--t-fast)",
            whiteSpace: "nowrap",
          }}
        >
          <ChevronDownIcon style={{ width: 10, height: 10 }} />
          {`+${siblingCount}`}
        </button>
      )}
    </span>
  );
}

// ── PickerRow ─────────────────────────────────────────────────────────────────

/** One compact row of a strip dropdown: state dot + name, optionally
 * depth-indented (mini-subtree pickers) or with a lineage breadcrumb
 * subtitle (quick-jump). */
function PickerRow({
  scenario,
  depth = 0,
  subtitle,
  isStale,
  isActive = false,
  onSelect,
}: {
  scenario: ScenarioDto;
  depth?: number;
  subtitle?: string;
  isStale: boolean;
  isActive?: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--nav-hover)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = "transparent";
      }}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 7,
        width: "100%",
        padding: `6px 12px 6px ${12 + depth * 14}px`,
        border: "none",
        background: "transparent",
        color: isActive ? "var(--accent)" : "var(--text-primary)",
        cursor: "pointer",
        fontSize: 12,
        textAlign: "left",
        fontFamily: "var(--font-ui)",
        transition: "background var(--t-fast)",
      }}
    >
      <span
        style={{
          width: 6,
          height: 6,
          borderRadius: "50%",
          background: stateColor(scenario.state, isStale),
          flexShrink: 0,
        }}
      />
      <span style={{ flex: 1, minWidth: 0 }}>
        <span
          style={{
            display: "block",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            fontWeight: isActive ? 600 : 400,
          }}
        >
          {scenario.name}
        </span>
        {subtitle && (
          <span
            style={{
              display: "block",
              fontSize: 10,
              color: "var(--text-tertiary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              marginTop: 1,
            }}
          >
            {subtitle}
          </span>
        )}
      </span>
      <span
        style={{
          fontSize: 10,
          color: "var(--text-tertiary)",
          flexShrink: 0,
        }}
      >
        {stateLabel(scenario.state, isStale)}
      </span>
    </button>
  );
}
