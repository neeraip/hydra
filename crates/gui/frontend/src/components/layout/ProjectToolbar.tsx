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
  variantTail,
} from "../panels/ScenariosPanel/shared";
import { PrimaryButton } from "../ui/PrimaryButton";
import { UnitSystemPicker } from "./UnitSystemPicker";

/* ─── ProjectToolbar ────────────────────────────────────────────────────────
   Persistent toolbar across the top of every project view. Holds scenario
   selection (a "Base" pill always present and selected by default when
   activeScenarioId === null) plus the run controls.

   The strip shows Base, every variant, and a one-level summary of what is
   below each variant — not the whole tree.

   Parentless scenarios are variants OF the base model rather than descendants
   of it, so they render beside Base as peers, with no arrow between (that edge
   is not descent) and no sibling picker (nothing is hidden to reveal). Each
   variant then carries a summary of its own subtree, separated from the next
   variant by a divider:

     no children        → nothing
     one leaf child     → → [that child]      (a chip is cheaper than a click)
     anything else      → → [▾ N children]

   One exception keeps the strip able to answer "where am I". The pickers list
   each child with all of its descendants, so a single click can activate a
   scenario at any depth — which a fixed two-level summary could never show. So
   the variant whose subtree holds the active scenario expands instead to the
   real path down to it: one chip per level, a true parent→child arrow between
   each, a ▾ "+N" segment on chips with siblings, and a trailing ▾ stub to keep
   descending. Every other variant stays summarised. Everything else lives in
   the Scenarios modal.

   Layout (left → right):
     [Scenarios ▸ 🔍] [Base pill] [variant] → [summary] │ [variant] → [active
     path… → ▾ stub]   [units] | [Simulate ▸ ⚙]

   The Scenarios split button leads rather than trails: it names the strip
   that follows, the way a section heading does, and the strip is the part
   that clips when scenarios are many — so the controls sit on the side
   that never scrolls out of reach.
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

/** Divider between one variant's group and the next. Variants are peers with
 * no arrow between them, so once a group can carry a trailing child or picker
 * the eye needs a boundary — otherwise `[a] → [a1] [b]` reads as though `b`
 * hung off `a1`. */
function VariantSeparator() {
  return (
    <span
      aria-hidden
      style={{
        flexShrink: 0,
        width: 1,
        height: 16,
        background: "var(--border)",
        margin: "0 2px",
      }}
    />
  );
}

/** Trailing "▾ N children" segment: opens a picker listing that branch point's
 * children with all their descendants, indented. */
function ChildrenStub({
  count,
  open,
  onToggle,
}: {
  count: number;
  open: boolean;
  onToggle: (rect: DOMRect) => void;
}) {
  return (
    <button
      type="button"
      data-scenario-picker
      data-tooltip="Descend to a child scenario"
      data-tooltip-pos="bottom"
      onClick={(e) => onToggle(e.currentTarget.getBoundingClientRect())}
      style={{
        flexShrink: 0,
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "4px 9px",
        border: "1px dashed var(--border-hover)",
        borderRadius: 14,
        background: open ? "var(--nav-hover)" : "transparent",
        color: "var(--text-tertiary)",
        fontSize: "var(--text-sm)",
        fontWeight: 500,
        cursor: "pointer",
        fontFamily: "var(--font-ui)",
        whiteSpace: "nowrap",
        transition: "background var(--t-fast)",
      }}
    >
      <ChevronDownIcon style={{ width: 10, height: 10 }} />
      {count} {count === 1 ? "child" : "children"}
    </button>
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
  const { project } = useActiveProject();
  const { isEdited, markEdited } = useNetworkVersion();

  const rawDtos = useScenarios(project?.id ?? null, scenariosVersion);
  const hasScenarios = rawDtos.length > 0;

  // Active path Base → … → active. Empty when Base is active or the stored
  // id is stale/unknown (the reset effect below then falls back to Base).
  const lineage = useMemo(
    () => activeLineage(rawDtos, activeScenarioId),
    [rawDtos, activeScenarioId],
  );
  // Parentless scenarios are variants OF the base model, not descendants of
  // it, so they sit beside it as peers rather than hiding behind one chip's
  // sibling picker.
  const variants = useMemo(() => scenarioChildren(rawDtos, null), [rawDtos]);

  // The variant whose subtree holds the active scenario, when the active
  // scenario is deeper than the variant itself. That one group expands to the
  // real path; every other group stays summarised. Without this the strip
  // caps out at depth 2 and a deep active scenario would appear nowhere in
  // it — no "you are here", and no ancestor chips to climb back up.
  const expandedVariantId = lineage.length > 1 ? lineage[0].id : null;

  // What each *unexpanded* variant shows after itself. Precomputed per variant
  // so the render pass doesn't re-scan the scenario list per chip.
  const variantTails = useMemo(
    () => new Map(variants.map((v) => [v.id, variantTail(rawDtos, v.id)])),
    [rawDtos, variants],
  );

  // Children of the active scenario — the trailing ▾ stub that continues the
  // expanded branch downward. Only meaningful inside that branch: computed for
  // Base it would list the variants again, duplicating the row beside it.
  const activeChildren = useMemo(
    () =>
      expandedVariantId ? scenarioChildren(rawDtos, activeScenarioId) : [],
    [rawDtos, activeScenarioId, expandedVariantId],
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
      {/* Scenarios — names the strip that follows *and* opens it, in the
          space a static "SCENARIOS" label used to occupy. A label that only
          names a region spends toolbar width on a word; this spends the
          same width on a word that also acts.

          Leading, not trailing, for the same reason a section heading
          precedes its section — and because the strip beside it is the part
          that gets clipped when scenarios are many, so the controls belong
          on the side that never scrolls away.

          Split like the Simulate button opposite it: the wide segment does
          the obvious thing, the narrow one an adjacent thing. */}
      <div
        style={{
          flexShrink: 0,
          display: "inline-flex",
          alignItems: "stretch",
          height: 26,
        }}
      >
        <button
          type="button"
          onClick={() => openScenariosModal()}
          data-tooltip="Manage scenarios"
          data-tooltip-pos="bottom"
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--nav-hover)";
            (e.currentTarget as HTMLButtonElement).style.color =
              "var(--text-primary)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background =
              "transparent";
            (e.currentTarget as HTMLButtonElement).style.color =
              "var(--text-secondary)";
          }}
          style={{
            border: "1px solid var(--border)",
            borderRight: hasScenarios ? "none" : "1px solid var(--border)",
            borderRadius: hasScenarios ? "5px 0 0 5px" : 5,
            background: "transparent",
            color: "var(--text-secondary)",
            padding: "0 9px",
            fontSize: "var(--text-sm)",
            fontWeight: 600,
            fontFamily: "var(--font-ui)",
            cursor: "pointer",
            transition: "background var(--t-fast), color var(--t-fast)",
          }}
        >
          Scenarios
        </button>
        {/* Nothing to search until a scenario exists — the segment is absent
            rather than disabled, and the label rounds off on its own. */}
        {hasScenarios && (
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
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                picker?.kind === "search" ? "var(--nav-hover)" : "transparent";
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-secondary)";
            }}
            style={{
              border: "1px solid var(--border)",
              borderRadius: "0 5px 5px 0",
              background:
                picker?.kind === "search" ? "var(--nav-hover)" : "transparent",
              color: "var(--text-secondary)",
              padding: "0 8px",
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              transition: "background var(--t-fast), color var(--t-fast)",
            }}
          >
            <MagnifyingGlassIcon style={{ width: 12, height: 12 }} />
          </button>
        )}
      </div>

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
            ? "1px solid var(--accent)"
            : "1px solid var(--border)",
          borderRadius: 14,
          background: baseActive
            ? "var(--selection-bg-strong)"
            : "var(--bg-card)",
          color: baseActive ? "var(--accent)" : "var(--text-secondary)",
          fontSize: "var(--text-sm)",
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

      {/* Scrollable lineage strip with right-edge fade. Every group hangs off a
          variant, so no variants means nothing to show. */}
      {variants.length > 0 && (
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
            {/* One group per variant: the variant chip, then either a summary
                of what is below it, or — for the one branch the user is
                actually in — the real path to the active scenario. */}
            {variants.map((v, i) => {
              const expanded = v.id === expandedVariantId;
              const tail = expanded ? null : variantTails.get(v.id);
              return (
                <Fragment key={v.id}>
                  {i > 0 && <VariantSeparator />}
                  <ScenarioChip
                    scenario={v}
                    isActive={v.id === activeScenarioId}
                    isAncestor={expanded}
                    isStale={isEdited(project.id, v.id) || v.state === "stale"}
                    siblingCount={0}
                    pickerOpen={false}
                    onClick={() => setActiveScenarioId(v.id)}
                    onTogglePicker={() => {}}
                  />

                  {/* Expanded branch. Below a variant it is genuine descent, so
                      arrows and sibling pickers apply as before. */}
                  {expanded &&
                    lineage.slice(1).map((s) => {
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
                            isAncestor={s.id !== activeScenarioId}
                            isStale={
                              isEdited(project.id, s.id) || s.state === "stale"
                            }
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

                  {/* Continue the expanded branch downward. Without this,
                      walking into a child you just left would need the Manage
                      modal. */}
                  {expanded && activeChildren.length > 0 && (
                    <>
                      <LineageArrow />
                      <ChildrenStub
                        count={activeChildren.length}
                        open={
                          picker?.kind === "children" &&
                          picker.id === activeScenarioId
                        }
                        onToggle={(rect) =>
                          togglePicker(
                            { kind: "children", id: activeScenarioId },
                            rect,
                          )
                        }
                      />
                    </>
                  )}

                  {/* Summary for a branch the user is not in. */}
                  {tail?.kind === "child" && (
                    <>
                      <LineageArrow />
                      <ScenarioChip
                        scenario={tail.child}
                        isActive={tail.child.id === activeScenarioId}
                        isAncestor={false}
                        isStale={
                          isEdited(project.id, tail.child.id) ||
                          tail.child.state === "stale"
                        }
                        siblingCount={0}
                        pickerOpen={false}
                        onClick={() => setActiveScenarioId(tail.child.id)}
                        onTogglePicker={() => {}}
                      />
                    </>
                  )}
                  {tail?.kind === "dropdown" && (
                    <>
                      <LineageArrow />
                      <ChildrenStub
                        count={tail.count}
                        open={picker?.kind === "children" && picker.id === v.id}
                        onToggle={(rect) =>
                          togglePicker({ kind: "children", id: v.id }, rect)
                        }
                      />
                    </>
                  )}
                </Fragment>
              );
            })}
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
              fontSize: "var(--text-md)",
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
                  fontSize: "var(--text-sm)",
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

      {/* Display units — grouped with the scenario controls because both
          answer "what am I looking at", and deliberately not beside
          Simulate, where it would read as "run in these units". */}
      <UnitSystemPicker />

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
  isAncestor = false,
  isStale,
  siblingCount,
  pickerOpen,
  onClick,
  onTogglePicker,
}: {
  scenario: ScenarioDto;
  isActive: boolean;
  /** On the active path but not the active scenario. Only the variant row
   * can be this: every chip after it descends from the active one, so before
   * that row existed every chip in the strip was on the path by
   * construction. Without it, a strip reading `[Base] [A] [B] [C] → …` gives
   * no clue which variant the arrow descends from. */
  isAncestor?: boolean;
  isStale: boolean;
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
  // Selection is the app's own colour, not the engine's. Which engine a
  // project uses is an identity, and identity belongs on identity marks —
  // spending it on "this one is selected" says the wrong thing twice, and
  // leaves the Base pill (which has always used the app accent) looking
  // like a different kind of control from its own siblings.
  const textColor = isActive ? "var(--accent)" : "var(--text-primary)";
  // Ancestors take the accent border without the fill: present on the path,
  // but not the thing selected.
  const borderColor =
    isActive || isAncestor ? "var(--accent)" : "var(--border)";

  return (
    <span
      style={{
        flexShrink: 0,
        display: "inline-flex",
        alignItems: "stretch",
        border: `1px solid ${borderColor}`,
        borderRadius: 14,
        background: isActive ? "var(--accent-dim)" : "var(--bg-card)",
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
          fontSize: "var(--text-sm)",
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
              ? "1px solid var(--accent)"
              : "1px solid var(--border)",
            background: pickerOpen ? "var(--nav-hover)" : "transparent",
            color: isActive ? "var(--accent)" : "var(--text-tertiary)",
            fontSize: "var(--text-xs)",
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
        fontSize: "var(--text-md)",
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
              fontSize: "var(--text-xs)",
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
          fontSize: "var(--text-xs)",
          color: "var(--text-tertiary)",
          flexShrink: 0,
        }}
      >
        {stateLabel(scenario.state, isStale)}
      </span>
    </button>
  );
}
