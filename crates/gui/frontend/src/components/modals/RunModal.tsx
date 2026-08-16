import { Cog6ToothIcon, PlayIcon, XMarkIcon } from "@heroicons/react/16/solid";
import type React from "react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { engineComponents } from "../../engine/registry";
import {
  enqueueRuns,
  fetchValidationFindings,
  getSimParams,
  projectHasNetwork,
  type SimParams,
  useScenarios,
} from "../../hooks";
import { fetchInto } from "../../hooks/fetchInto";
import { formatIpcError } from "../../hooks/ipc";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import {
  formatShortcut,
  primaryModifierLabel,
  primaryModifierPressed,
} from "../../shortcuts";
import { DialogButton } from "../ui/DialogButton";
import { EngineGlyph } from "../ui/EngineGlyph";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";
import {
  ActiveBadge,
  Label,
  runnableScenarioIds,
  SimStateBadge,
} from "./RunModal/helpers";

// ─────────────────────────────────────────────────────────────────────────────
// Run modal — read-only.
//
// Simulation parameters are owned by the Simulation Settings modal (which writes
// them back to the base/model.inp). This modal just displays the resolved
// settings, lets the engineer pick which scenario to run against, and runs.
// To change duration/timesteps/quality mode, click "Edit settings" → opens the
// Simulation Settings modal.
// ─────────────────────────────────────────────────────────────────────────────

interface ScenarioOption {
  /** null = base model */
  id: string | null;
  label: string;
  /** "not-run" | "simulated" | "stale" | "running" | "failed" | "queued" | "draft" | "ready" */
  state: string;
}

/** Any state that isn't a valid simulation is considered outdated. */
const isOutdated = (state: string) => state !== "simulated";

const linkBtn: React.CSSProperties = {
  background: "transparent",
  border: "none",
  padding: 0,
  fontSize: "var(--text-sm)",
  cursor: "pointer",
  fontFamily: "var(--font-ui)",
};

function ScenarioRow({
  scenario,
  isChecked,
  isActive,
  isLast,
  errorCount,
  onToggle,
}: {
  scenario: ScenarioOption;
  isChecked: boolean;
  isActive: boolean;
  isLast: boolean;
  /** Blocking validation errors; > 0 means the solver would reject it. */
  errorCount: number;
  onToggle: () => void;
}) {
  const blocked = errorCount > 0;
  return (
    <label
      title={
        blocked
          ? `This model has ${errorCount} error${errorCount === 1 ? "" : "s"} and cannot be simulated. See the Issues panel.`
          : undefined
      }
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "8px 12px",
        borderBottom: isLast ? "none" : "1px solid var(--border)",
        cursor: blocked ? "not-allowed" : "pointer",
        background:
          isChecked && !blocked ? "rgba(100,160,255,0.06)" : "transparent",
        opacity: blocked ? 0.55 : 1,
        transition: "background 80ms",
      }}
    >
      <input
        type="checkbox"
        checked={isChecked && !blocked}
        disabled={blocked}
        onChange={onToggle}
        style={{
          accentColor: "var(--accent)",
          width: 13,
          height: 13,
          flexShrink: 0,
        }}
      />
      <span
        style={{
          flex: 1,
          fontSize: "var(--text-lg)",
          color: blocked ? "var(--text-tertiary)" : "var(--text-primary)",
          fontFamily: "var(--font-ui)",
        }}
      >
        {scenario.label}
      </span>
      {isActive && <ActiveBadge />}
      {blocked ? (
        <span
          className="badge"
          style={{
            color: "var(--status-error)",
            background:
              "color-mix(in srgb, var(--status-error) 15%, transparent)",
            borderColor:
              "color-mix(in srgb, var(--status-error) 35%, transparent)",
            fontWeight: 600,
          }}
        >
          {errorCount} error{errorCount === 1 ? "" : "s"}
        </span>
      ) : (
        <SimStateBadge state={scenario.state} />
      )}
    </label>
  );
}

export function RunModal() {
  const {
    runModalOpen,
    closeRunModal,
    openTaskTray,
    activeProjectId,
    activeScenarioId,
    openSimSettingsModal,
    scenariosVersion,
    showToast,
  } = useAppState();
  const { project, engine } = useActiveProject();
  const { isEdited, version: networkVersion } = useNetworkVersion();

  const dbScenarios = useScenarios(activeProjectId ?? null, scenariosVersion);
  const scenarios: ScenarioOption[] = useMemo(
    () => [
      {
        id: null,
        label: "Base",
        state: isEdited(project?.id ?? null, null)
          ? "stale"
          : (project?.state ?? "not-run"),
      },
      ...dbScenarios.map((s) => ({
        id: s.id,
        label: s.name,
        state: isEdited(project?.id ?? null, s.id) ? "stale" : s.state,
      })),
    ],
    [dbScenarios, project?.id, project?.state, isEdited],
  );

  // Checked set — stored as the same id representation used in scenarios list.
  const [checked, setChecked] = useState<Set<string | null>>(
    new Set([activeScenarioId]),
  );
  const [params, setParams] = useState<SimParams | null>(null);
  const checkedIds = useMemo(() => [...checked], [checked]);
  const hasNetwork = projectHasNetwork(project);

  // When the modal opens, reset the checklist to just the active scenario.
  useEffect(() => {
    if (runModalOpen) setChecked(new Set([activeScenarioId]));
  }, [runModalOpen, activeScenarioId]);

  // Refetch sim params whenever the modal opens (the Overview page may have
  // edited them since last open) and when the active project changes.
  useEffect(() => {
    if (!runModalOpen || !activeProjectId) return;
    return fetchInto(getSimParams(activeProjectId), setParams);
  }, [runModalOpen, activeProjectId]);

  // Blocking validation errors per scenario id. A scenario the solver would
  // reject cannot be queued, so the modal has to know before offering it —
  // `validationIssues` in SimulationContext covers only the *active*
  // scenario, and this modal runs a set.
  const [errorCounts, setErrorCounts] = useState<Map<string | null, number>>(
    new Map(),
  );
  const scenarioIds = useMemo(() => scenarios.map((s) => s.id), [scenarios]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `networkVersion` is an intentional retrigger — revalidate after structural edits.
  useEffect(() => {
    if (!runModalOpen || !activeProjectId) return;
    return fetchInto(
      Promise.all(
        scenarioIds.map(async (id) => {
          const findings = await fetchValidationFindings(activeProjectId, id);
          const errors = findings.filter((f) => f.severity === "error").length;
          return [id, errors] as const;
        }),
      ),
      (entries) => setErrorCounts(new Map(entries)),
    );
  }, [runModalOpen, activeProjectId, scenarioIds, networkVersion]);

  const errorsFor = useCallback(
    (id: string | null) => errorCounts.get(id) ?? 0,
    [errorCounts],
  );
  // What will actually be queued: checked, minus anything the solver would
  // reject.
  const runnableIds = useMemo(
    () => runnableScenarioIds(checkedIds, errorCounts),
    [checkedIds, errorCounts],
  );

  const runSimulation = useCallback(() => {
    if (!activeProjectId || runnableIds.length === 0) return;
    closeRunModal();
    // openTaskTray (not toggle): if the tray is already open, toggling would
    // close it just as the queued runs start.
    setTimeout(() => openTaskTray(), 200);
    enqueueRuns(activeProjectId, runnableIds).catch((err) => {
      showToast(`Failed to queue runs: ${formatIpcError(err)}`, "error");
    });
  }, [activeProjectId, runnableIds, closeRunModal, openTaskTray, showToast]);

  // Esc closes; Cmd/Ctrl+Enter runs.
  useEffect(() => {
    if (!runModalOpen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        closeRunModal();
      }
      if (primaryModifierPressed(e) && e.key === "Enter") {
        e.preventDefault();
        runSimulation();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [runModalOpen, runSimulation, closeRunModal]);

  if (!runModalOpen) return null;

  const runShortcut = formatShortcut([primaryModifierLabel(), "Enter"]);

  // The settings card's body is the engine's own component (registry) —
  // this modal owns only the card chrome and the edit affordance. Engines
  // whose settings are not editable never gate running on a params object.
  const components = engineComponents(engine?.key);
  const settingsSupported = components.settingsEditable;

  // A project with no network has nothing to simulate — the engine would be
  // handed an empty model and fail at parse time. Scenarios with blocking
  // validation errors are excluded rather than blocking the whole run: a
  // sibling's broken model should not stop a valid one being simulated.
  const canRun =
    hasNetwork &&
    (params != null || !settingsSupported) &&
    runnableIds.length > 0;
  const excludedCount = checkedIds.length - runnableIds.length;
  const allChecked = scenarios.every((s) => checked.has(s.id));

  function toggleScenario(id: string | null) {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleAll() {
    if (allChecked) setChecked(new Set());
    else setChecked(new Set(scenarios.map((s) => s.id)));
  }

  function selectOutdated() {
    setChecked(
      new Set(scenarios.filter((s) => isOutdated(s.state)).map((s) => s.id)),
    );
  }

  const hasOutdated = scenarios.some((s) => isOutdated(s.state));

  const runLabel =
    checkedIds.length === 0
      ? "Run"
      : allChecked
        ? `Run All (${scenarios.length})`
        : checkedIds.length === 1
          ? "Run"
          : `Run ${checkedIds.length}`;

  function goEditSettings() {
    // openSimSettingsModal closes this run modal as part of its state update.
    openSimSettingsModal();
  }

  return (
    <ModalBackdrop
      onDismiss={closeRunModal}
      zIndex={200}
      style={{ animation: "fadeIn 120ms ease-out" }}
    >
      {/* This modal has an engine to speak for, so it says so. It mounts at
          the app root rather than inside the project, which is why the
          variable does not simply reach it — position in the tree is not
          the same question as whether a surface belongs to an engine. */}
      <div
        {...stopBackdropEvents}
        style={{
          ...(engine?.accent
            ? ({
                "--engine-accent": engine.accent,
                "--engine-accent-fg": "#fff",
              } as React.CSSProperties)
            : null),
          width: "100%",
          maxWidth: 560,
          maxHeight: "82vh",
          background: "var(--bg-panel)",
          backdropFilter: "blur(24px)",
          border: "1px solid var(--border-hover)",
          borderRadius: 12,
          boxShadow: "var(--shadow-3)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          animation: "scaleIn 160ms ease-out",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "14px 20px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <EngineGlyph engine={engine} />
          <div style={{ flex: 1 }}>
            <div
              style={{
                fontSize: "var(--text-xl)",
                fontWeight: 600,
                color: "var(--text-primary)",
              }}
            >
              Run Simulation
            </div>
            <div
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
              }}
            >
              {project?.name ?? "(no project)"}
            </div>
          </div>
          <button
            type="button"
            className="tl-btn"
            onClick={closeRunModal}
            data-tooltip="Close (Esc)"
            aria-label="Close"
            style={{
              width: 26,
              height: 26,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        {/* Body */}
        <div style={{ flex: 1, overflowY: "auto", padding: "16px 20px" }}>
          {!hasNetwork && (
            <div
              style={{
                display: "flex",
                gap: 10,
                alignItems: "flex-start",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                borderRadius: 6,
                padding: "10px 12px",
                marginBottom: 16,
                fontSize: "var(--text-md)",
                color: "var(--text-secondary)",
                lineHeight: 1.6,
              }}
            >
              <span style={{ flexShrink: 0, fontSize: "var(--text-xl)" }}>
                ℹ
              </span>
              <span>
                This project has no network yet. Import a model file or build
                one in the editor before running a simulation.
              </span>
            </div>
          )}

          {/* Scenario checklist */}
          <div style={{ marginBottom: 16 }}>
            <div
              style={{
                display: "flex",
                alignItems: "baseline",
                justifyContent: "space-between",
                marginBottom: 8,
              }}
            >
              <Label>Simulate</Label>
              <div style={{ display: "flex", gap: 10 }}>
                {hasOutdated && (
                  <button
                    type="button"
                    onClick={selectOutdated}
                    style={{ ...linkBtn, color: "var(--text-secondary)" }}
                  >
                    Select outdated
                  </button>
                )}
                {scenarios.length > 1 && (
                  <button
                    type="button"
                    onClick={toggleAll}
                    style={{ ...linkBtn, color: "var(--accent)" }}
                  >
                    {allChecked ? "Deselect all" : "Select all"}
                  </button>
                )}
              </div>
            </div>
            <div
              style={{
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                borderRadius: 6,
                overflow: "hidden",
                maxHeight: 224,
                overflowY: "auto",
              }}
            >
              {/* Base row */}
              <ScenarioRow
                scenario={scenarios[0]}
                isChecked={checked.has(scenarios[0].id)}
                isActive={scenarios[0].id === activeScenarioId}
                isLast={scenarios.length === 1}
                errorCount={errorsFor(scenarios[0].id)}
                onToggle={() => toggleScenario(scenarios[0].id)}
              />

              {/* Scenarios section header + rows */}
              {scenarios.length > 1 && (
                <>
                  <div
                    style={{
                      padding: "5px 12px",
                      fontSize: "var(--text-xs)",
                      fontWeight: 700,
                      letterSpacing: "0.06em",
                      color: "var(--text-tertiary)",
                      textTransform: "uppercase",
                      background: "var(--bg-panel)",
                      borderBottom: "1px solid var(--border)",
                      userSelect: "none",
                    }}
                  >
                    Scenarios
                  </div>
                  {scenarios.slice(1).map((s, i) => (
                    <ScenarioRow
                      key={s.id}
                      scenario={s}
                      isChecked={checked.has(s.id)}
                      isActive={s.id === activeScenarioId}
                      isLast={i === scenarios.length - 2}
                      errorCount={errorsFor(s.id)}
                      onToggle={() => toggleScenario(s.id)}
                    />
                  ))}
                </>
              )}
            </div>
          </div>

          {/* Read-only sim params summary */}
          <div
            style={{
              display: "flex",
              alignItems: "baseline",
              justifyContent: "space-between",
              marginBottom: 8,
            }}
          >
            <Label>Simulation settings</Label>
            <button
              type="button"
              onClick={goEditSettings}
              style={{
                background: "transparent",
                border: "none",
                padding: 0,
                color: "var(--accent)",
                fontSize: "var(--text-sm)",
                cursor: "pointer",
                fontFamily: "var(--font-ui)",
                display: "inline-flex",
                alignItems: "center",
                gap: 3,
              }}
              data-tooltip="Open simulation settings"
            >
              <Cog6ToothIcon style={{ width: 11, height: 11 }} />
              Edit settings
            </button>
          </div>
          {activeProjectId ? (
            <components.RunSettingsSummary projectId={activeProjectId} />
          ) : (
            <div
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
              }}
            >
              No project selected.
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "12px 20px",
            borderTop: "1px solid var(--border)",
            background: "rgba(0,0,0,0.18)",
          }}
        >
          {/* Say plainly when the run is smaller than what was ticked — the
              alternative is pressing Simulate for two scenarios and silently
              getting one. */}
          <div style={{ flex: 1, fontSize: "var(--text-md)" }}>
            {excludedCount > 0 && (
              <span style={{ color: "var(--text-secondary)" }}>
                {runnableIds.length === 0
                  ? `${excludedCount === 1 ? "This model has" : "These models have"} errors and cannot be simulated`
                  : `${runnableIds.length} of ${checkedIds.length} will run — ${excludedCount} ${excludedCount === 1 ? "has" : "have"} errors`}
              </span>
            )}
          </div>
          <DialogButton onClick={closeRunModal}>Cancel</DialogButton>
          <button
            type="button"
            onClick={runSimulation}
            disabled={!canRun}
            aria-label="Run simulation"
            data-tooltip={
              canRun
                ? `Run (${runShortcut})`
                : checkedIds.length === 0
                  ? "Select a scenario"
                  : "No model loaded"
            }
            style={{
              background: canRun
                ? "var(--engine-accent, var(--accent))"
                : "var(--bg-card)",
              // The fill's own colour, not the achromatic accent. Left as
              // it was, an engine-coloured button wore a near-white outline.
              border: `1px solid ${
                canRun ? "var(--engine-accent, var(--accent))" : "var(--border)"
              }`,
              // White on the accent was legible while the accent was a
              // saturated blue; achromatic it is a light grey, and white
              // text on it barely reads. `--accent-fg` is the pair the
              // accent publishes for exactly this.
              color: canRun
                ? "var(--engine-accent-fg, var(--accent-fg))"
                : "var(--text-disabled)",
              borderRadius: 5,
              padding: "7px 16px",
              fontSize: "var(--text-md)",
              fontWeight: 600,
              cursor: canRun ? "pointer" : "not-allowed",
              opacity: canRun ? 1 : 0.6,
              fontFamily: "var(--font-ui)",
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              transition: "filter var(--t-fast)",
            }}
            onMouseEnter={(e) => {
              if (canRun) e.currentTarget.style.filter = "brightness(1.12)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.filter = "";
            }}
          >
            <PlayIcon style={{ width: 14, height: 14 }} /> {runLabel}
            <span
              style={{
                fontSize: "var(--text-xs)",
                opacity: 0.85,
                fontFamily: "var(--font-mono)",
              }}
            >
              {runShortcut}
            </span>
          </button>
        </div>
      </div>
    </ModalBackdrop>
  );
}
