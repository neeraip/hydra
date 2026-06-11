import { Cog6ToothIcon, PlayIcon, XMarkIcon } from "@heroicons/react/16/solid";
import type React from "react";
import { useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import {
  ACCENT,
  enqueueRuns,
  getSimParams,
  PILL,
  type SimParams,
  useScenarios,
} from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import {
  ActiveBadge,
  Label,
  SimStateBadge,
  SummaryGrid,
} from "./RunModal/helpers";

// ─────────────────────────────────────────────────────────────────────────────
// Run modal — read-only.
//
// Simulation parameters are owned by the Project Overview page (which writes
// them back to the base/model.inp). This modal just displays the resolved
// settings, lets the engineer pick which scenario to run against, and runs.
// To change duration/timesteps/quality mode, click "Edit settings" → Overview.
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
  fontSize: 11,
  cursor: "pointer",
  fontFamily: "var(--font-ui)",
};

function ScenarioRow({
  scenario,
  isChecked,
  isActive,
  isLast,
  onToggle,
}: {
  scenario: ScenarioOption;
  isChecked: boolean;
  isActive: boolean;
  isLast: boolean;
  onToggle: () => void;
}) {
  return (
    <label
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "8px 12px",
        borderBottom: isLast ? "none" : "1px solid var(--border)",
        cursor: "pointer",
        background: isChecked ? "rgba(100,160,255,0.06)" : "transparent",
        transition: "background 80ms",
      }}
    >
      <input
        type="checkbox"
        checked={isChecked}
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
          fontSize: 13,
          color: "var(--text-primary)",
          fontFamily: "var(--font-ui)",
        }}
      >
        {scenario.label}
      </span>
      {isActive && <ActiveBadge />}
      <SimStateBadge state={scenario.state} />
    </label>
  );
}

export function RunModal() {
  const {
    runModalOpen,
    closeRunModal,
    toggleTaskTray,
    activeProjectId,
    activeScenarioId,
    setProjectView,
    scenariosVersion,
  } = useAppState();
  const { project } = useActiveProject();
  const { editedScenarioIds } = useNetworkVersion();

  const dbScenarios = useScenarios(activeProjectId ?? null, scenariosVersion);
  const scenarios: ScenarioOption[] = useMemo(
    () => [
      {
        id: null,
        label: "Base",
        state: editedScenarioIds.has(null)
          ? "stale"
          : (project?.state ?? "not-run"),
      },
      ...dbScenarios.map((s) => ({
        id: s.id,
        label: s.name,
        state: editedScenarioIds.has(s.id) ? "stale" : s.state,
      })),
    ],
    [dbScenarios, project?.state, editedScenarioIds],
  );

  // Checked set — stored as the same id representation used in scenarios list.
  const [checked, setChecked] = useState<Set<string | null>>(
    new Set([activeScenarioId]),
  );
  const [params, setParams] = useState<SimParams | null>(null);

  // When the modal opens, reset the checklist to just the active scenario.
  useEffect(() => {
    if (runModalOpen) setChecked(new Set([activeScenarioId]));
  }, [runModalOpen, activeScenarioId]);

  // Refetch sim params whenever the modal opens (the Overview page may have
  // edited them since last open) and when the active project changes.
  useEffect(() => {
    if (!runModalOpen || !activeProjectId) return;
    let cancelled = false;
    getSimParams(activeProjectId).then((p) => {
      if (!cancelled) setParams(p);
    });
    return () => {
      cancelled = true;
    };
  }, [runModalOpen, activeProjectId]);

  // Esc closes; Cmd/Ctrl+Enter runs.
  useEffect(() => {
    if (!runModalOpen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        closeRunModal();
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault();
        runSimulation();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runModalOpen, runSimulation, closeRunModal]);

  if (!runModalOpen) return null;

  const checkedIds = [...checked];
  const canRun = params != null && checkedIds.length > 0;
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

  function runSimulation() {
    if (!activeProjectId || checkedIds.length === 0) return;
    closeRunModal();
    setTimeout(() => toggleTaskTray(), 200);
    enqueueRuns(activeProjectId, checkedIds);
  }

  const runLabel =
    checkedIds.length === 0
      ? "Run"
      : allChecked
        ? `Run All (${scenarios.length})`
        : checkedIds.length === 1
          ? "Run"
          : `Run ${checkedIds.length}`;

  function goEditSettings() {
    closeRunModal();
    setProjectView("overview");
  }

  return (
    <div
      onClick={closeRunModal}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--bg-overlay)",
        zIndex: 200,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        animation: "fadeIn 120ms ease-out",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
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
          <span
            style={{
              fontSize: 11,
              fontWeight: 700,
              letterSpacing: "0.06em",
              color: ACCENT,
              background: `${ACCENT}26`,
              border: `1px solid ${ACCENT}55`,
              padding: "3px 8px",
              borderRadius: 4,
            }}
          >
            {PILL}
          </span>
          <div style={{ flex: 1 }}>
            <div
              style={{
                fontSize: 14,
                fontWeight: 600,
                color: "var(--text-primary)",
              }}
            >
              Run Simulation
            </div>
            <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              {project?.name ?? "(no project)"}
            </div>
          </div>
          <button
            className="tl-btn"
            onClick={closeRunModal}
            data-tooltip="Close (Esc)"
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
                    onClick={selectOutdated}
                    style={{ ...linkBtn, color: "var(--text-secondary)" }}
                  >
                    Select outdated
                  </button>
                )}
                {scenarios.length > 1 && (
                  <button
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
                onToggle={() => toggleScenario(scenarios[0].id)}
              />

              {/* Scenarios section header + rows */}
              {scenarios.length > 1 && (
                <>
                  <div
                    style={{
                      padding: "5px 12px",
                      fontSize: 10,
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
              onClick={goEditSettings}
              style={{
                background: "transparent",
                border: "none",
                padding: 0,
                color: "var(--accent)",
                fontSize: 11,
                cursor: "pointer",
                fontFamily: "var(--font-ui)",
                display: "inline-flex",
                alignItems: "center",
                gap: 3,
              }}
              data-tooltip="Open project overview to edit"
            >
              <Cog6ToothIcon style={{ width: 11, height: 11 }} />
              Edit on Overview
            </button>
          </div>
          {params ? (
            <SummaryGrid params={params} />
          ) : (
            <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
              {activeProjectId ? "Loading…" : "No project selected."}
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
          <div style={{ flex: 1 }} />
          <button
            onClick={closeRunModal}
            style={{
              background: "transparent",
              border: "1px solid var(--border)",
              color: "var(--text-secondary)",
              borderRadius: 5,
              padding: "7px 14px",
              fontSize: 12,
              cursor: "pointer",
              fontFamily: "var(--font-ui)",
            }}
          >
            Cancel
          </button>
          <button
            onClick={runSimulation}
            disabled={!canRun}
            data-tooltip={
              canRun
                ? "Run (⌘↵)"
                : checkedIds.length === 0
                  ? "Select a scenario"
                  : "No model loaded"
            }
            style={{
              background: canRun ? ACCENT : "var(--bg-card)",
              border: `1px solid ${canRun ? ACCENT : "var(--border)"}`,
              color: canRun ? "#fff" : "var(--text-disabled)",
              borderRadius: 5,
              padding: "7px 16px",
              fontSize: 12,
              fontWeight: 600,
              cursor: canRun ? "pointer" : "not-allowed",
              opacity: canRun ? 1 : 0.6,
              fontFamily: "var(--font-ui)",
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            <PlayIcon style={{ width: 14, height: 14 }} /> {runLabel}
            <span
              style={{
                fontSize: 10,
                opacity: 0.85,
                fontFamily: "var(--font-mono)",
              }}
            >
              ⌘↵
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}
