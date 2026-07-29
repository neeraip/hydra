/**
 * Confirmation and execution for "clear simulation results".
 *
 * Rendered once at app level and driven by `clearResults` in AppContext, so
 * every surface that offers the action — the Scenarios panel rows, the
 * command palette — funnels through the same confirmation, the same delete,
 * the same toasts and the same refresh. The palette in particular cannot own
 * this itself: it unmounts the instant it runs a command.
 *
 * Deleting results is destructive and not undoable — a cleared run has to be
 * re-run, which on a large network is minutes of work — so it is always
 * confirmed, never fired straight from a menu click.
 */

import { useEffect, useState } from "react";
import { useAppState, useSimulation } from "../../AppContext";
import {
  allSimulationResultsSize,
  deleteAllSimulations,
  deleteSimulation,
  simulationResultsSize,
} from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import { formatBytes } from "../../units";
import { DeleteConfirmModal } from "./DeleteConfirmModal";

export function ClearResultsModal() {
  const {
    clearResults: request,
    closeClearResults,
    activeScenarioId,
    bumpProjects,
    bumpScenarios,
    showToast,
  } = useAppState();
  const { setResultMeta, setPumpEnergy } = useSimulation();

  // Measured while the prompt is open rather than passed in: the callers
  // request a clear, they do not know what it costs, and a stale figure
  // would understate what is about to be deleted.
  const [bytes, setBytes] = useState<number | null>(null);
  const scope = request?.scope;
  const projectId = request?.projectId;
  const scenarioId = request?.scenarioId ?? null;
  useEffect(() => {
    if (!projectId || !scope) {
      setBytes(null);
      return;
    }
    let cancelled = false;
    setBytes(null);
    const pending =
      scope === "all"
        ? allSimulationResultsSize(projectId)
        : simulationResultsSize(projectId, scenarioId);
    void pending.then((n) => {
      if (!cancelled) setBytes(n);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, scope]);

  if (!request) return null;

  const all = request.scope === "all";
  // Omitted until measured, so the prompt never shows "0 bytes" for results
  // that simply have not been sized yet.
  const freed =
    bytes === null ? null : ` This frees ${formatBytes(bytes)} of disk space.`;
  // Only some callers know the count; the Scenarios panel has the scenario
  // list in hand, the command palette does not. Omit the sentence rather
  // than defaulting to a number, which would state something untrue.
  const simulatedCount = request.simulatedCount;

  async function run() {
    if (!request) return;
    closeClearResults();
    try {
      const removed = all
        ? await deleteAllSimulations(request.projectId)
        : (await deleteSimulation(request.projectId, request.scenarioId))
          ? 1
          : 0;

      bumpScenarios();
      bumpProjects();

      // Result metadata is loaded for the active target only, and otherwise
      // refreshed only on a target switch or a completed run. Clearing what
      // the user is currently looking at has to drop it here, or the canvas
      // keeps rendering a timeline whose results file no longer exists.
      if (all || request.scenarioId === activeScenarioId) {
        setResultMeta(null);
        setPumpEnergy(null);
      }

      if (removed === 0) {
        showToast(`"${request.name}" had no results to clear`, "info");
      } else if (all) {
        showToast(
          `Cleared results for ${removed} target${removed === 1 ? "" : "s"}`,
          "success",
        );
      } else {
        showToast(`Cleared results for "${request.name}"`, "success");
      }
    } catch (err) {
      showToast(`Could not clear results: ${formatIpcError(err)}`, "error");
    }
  }

  return (
    <DeleteConfirmModal
      open
      elementKind="results"
      elementId={request.name}
      title={all ? "Clear All Simulation Results" : "Clear Simulation Results"}
      message={
        all ? (
          <>
            Delete the simulation results for the base model and{" "}
            <strong style={{ color: "var(--text-primary)" }}>
              every scenario
            </strong>{" "}
            in this project?{" "}
            {simulatedCount !== undefined && (
              <>
                {simulatedCount} target{simulatedCount === 1 ? "" : "s"}{" "}
                currently hold results.{" "}
              </>
            )}
            The networks themselves are not changed, so the runs can be
            repeated.{freed}
          </>
        ) : (
          <>
            Delete the simulation results for{" "}
            <strong style={{ color: "var(--text-primary)" }}>
              {request.name}
            </strong>
            ? It returns to an unsimulated state. The network itself is not
            changed, so the run can be repeated.{freed}
          </>
        )
      }
      confirmLabel={all ? "Clear all results" : "Clear results"}
      onConfirm={run}
      onCancel={closeClearResults}
    />
  );
}
