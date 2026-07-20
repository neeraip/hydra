import {
  ArrowRightIcon,
  Cog6ToothIcon,
  PlayIcon,
} from "@heroicons/react/16/solid";
import { useEffect } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { type ScenarioDto, useScenarios } from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import { formatPrimaryShortcut } from "../../shortcuts";
import { PrimaryButton } from "../ui/PrimaryButton";

/* ─── ScenarioStrip ─────────────────────────────────────────────────────────
   Horizontal strip across the top of the canvas. A "Base" pill is always
   present and selected by default (activeScenarioId === null). Selecting a
   scenario pill deselects Base and vice-versa — only one can be active.

   Layout (left → right):
     [Base pill] | [scenario chip · …] · Manage
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

interface FlatScenario extends ScenarioDto {
  depth: number;
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

// ── Flatten scenario tree (BFS by depth) ─────────────────────────────────────

function flattenScenarios(dtos: ScenarioDto[]): FlatScenario[] {
  const byId = new Map(dtos.map((d) => [d.id, d]));
  const childrenOf = new Map<string | null, ScenarioDto[]>();
  for (const d of dtos) {
    const key = d.parentScenarioId ?? null;
    if (!childrenOf.has(key)) childrenOf.set(key, []);
    childrenOf.get(key)?.push(d);
  }
  const result: FlatScenario[] = [];
  const queue: Array<{ id: string; depth: number }> = (
    childrenOf.get(null) ?? []
  ).map((d) => ({
    id: d.id,
    depth: 0,
  }));
  while (queue.length > 0) {
    const next = queue.shift();
    if (!next) break;
    const { id, depth } = next;
    const dto = byId.get(id);
    if (!dto) continue;
    result.push({ ...dto, depth });
    for (const child of childrenOf.get(id) ?? []) {
      queue.push({ id: child.id, depth: depth + 1 });
    }
  }
  return result;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function ScenarioStrip() {
  const {
    openRunModal,
    openScenariosModal,
    activeScenarioId,
    setActiveScenarioId,
    scenariosVersion,
  } = useAppState();
  const { project, accent } = useActiveProject();
  const { editedScenarioIds, markEdited } = useNetworkVersion();

  const rawDtos = useScenarios(project?.id ?? null, scenariosVersion);
  const scenarios = flattenScenarios(rawDtos);

  // If the active scenario was deleted, fall back to Base.
  // Guard on scenarios.length > 0 so we don't reset before the list loads.
  useEffect(() => {
    if (
      activeScenarioId &&
      scenarios.length > 0 &&
      !scenarios.find((s) => s.id === activeScenarioId)
    ) {
      setActiveScenarioId(null);
    }
  }, [scenarios, activeScenarioId, setActiveScenarioId]);

  // Seed editedScenarioIds from DB-persisted stale state so the amber
  // indicators survive app restarts. markEdited is idempotent.
  useEffect(() => {
    for (const s of scenarios) {
      if (s.state === "stale") markEdited(s.id);
    }
  }, [scenarios, markEdited]);

  useEffect(() => {
    if (project?.state === "stale") markEdited(null);
  }, [project?.state, markEdited]);

  if (!project) return null;

  const activeScenario = activeScenarioId
    ? (scenarios.find((s) => s.id === activeScenarioId) ?? null)
    : null;

  const baseActive = activeScenarioId === null;
  const baseStale = editedScenarioIds.has(null) || project.state === "stale";
  const baseState = (project.state ?? "draft") as ScenarioState;
  const baseEffectiveColor = baseStale
    ? "#f59e0b"
    : (STATE_COLOR[baseState] ?? STATE_COLOR["not-run"]);
  const baseRunning = baseState === "running";
  const baseTitle = `Base model · ${STATE_LABEL[baseState] ?? baseState}${baseStale ? " · network edited since last run" : ""}`;

  // Active-scenario-scoped flags that drive the Run button appearance.
  const activeIsStale =
    activeScenarioId === null
      ? baseStale
      : editedScenarioIds.has(activeScenarioId) ||
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
    : `Configure & run simulation (${runShortcut})`;

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
      {/* Base pill — always present, active when no scenario is selected */}
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
          fontWeight: baseActive ? 700 : 500,
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
        Base model
      </button>

      <span
        style={{
          width: 1,
          height: 14,
          background: "var(--border)",
          flexShrink: 0,
        }}
      />

      {/* Section label */}
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

      {/* Scrollable scenario chip list with right-edge fade */}
      {scenarios.length > 0 && (
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
            {scenarios.map((s, i) => (
              <ScenarioChip
                key={s.id}
                scenario={s}
                isActive={s.id === activeScenarioId}
                isStale={editedScenarioIds.has(s.id)}
                isLast={i === scenarios.length - 1}
                accent={accent}
                onClick={() => setActiveScenarioId(s.id)}
              />
            ))}
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

      {/* Run button — marginLeft: auto pushes it to the far right */}
      <PrimaryButton
        size="sm"
        onClick={openRunModal}
        className={runBtnClass}
        data-tooltip={runBtnTitle}
        data-tooltip-pos="bottom"
        style={{
          marginLeft: "auto",
          flexShrink: 0,
          display: "inline-flex",
          alignItems: "center",
          gap: 5,
        }}
      >
        <PlayIcon style={{ width: 12, height: 12 }} />
        {runBtnLabel}
      </PrimaryButton>
    </div>
  );
}

// ── ScenarioChip ──────────────────────────────────────────────────────────────

function ScenarioChip({
  scenario,
  isActive,
  isStale,
  isLast,
  accent,
  onClick,
}: {
  scenario: FlatScenario;
  isActive: boolean;
  isStale: boolean;
  isLast: boolean;
  accent: string;
  onClick: () => void;
}) {
  const state = (scenario.state ?? "not-run") as ScenarioState;
  const effectiveColor = isStale
    ? "#f59e0b"
    : (STATE_COLOR[state] ?? STATE_COLOR["not-run"]);
  const isRunning = state === "running";
  const titleSuffix = isStale ? " · network edited since last run" : "";

  return (
    <>
      {scenario.depth > 0 && (
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
      )}

      <button
        type="button"
        onClick={onClick}
        data-tooltip={`${scenario.name} · ${STATE_LABEL[state]}${titleSuffix}`}
        data-tooltip-pos="bottom"
        onMouseEnter={(e) => {
          if (!isActive) {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--nav-hover)";
            (e.currentTarget as HTMLButtonElement).style.borderColor =
              "var(--border-hover)";
          }
        }}
        onMouseLeave={(e) => {
          if (!isActive) {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--bg-card)";
            (e.currentTarget as HTMLButtonElement).style.borderColor =
              "var(--border)";
          }
        }}
        style={{
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: 5,
          padding: "4px 9px 4px 7px",
          border: isActive ? `1px solid ${accent}` : "1px solid var(--border)",
          borderRadius: 14,
          background: isActive ? `${accent}22` : "var(--bg-card)",
          color: isActive ? accent : "var(--text-primary)",
          fontSize: 11,
          fontWeight: isActive ? 700 : 500,
          cursor: "pointer",
          fontFamily: "var(--font-ui)",
          transition: "background var(--t-fast), border-color var(--t-fast)",
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

      {/* Separator between sibling top-level scenarios */}
      {!isLast && scenario.depth === 0 && (
        <span
          style={{
            width: 1,
            height: 14,
            background: "var(--border)",
            flexShrink: 0,
          }}
        />
      )}
    </>
  );
}
