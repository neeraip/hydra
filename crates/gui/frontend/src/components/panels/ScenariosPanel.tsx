/* Scenario management page — create, rename, branch, run, and delete scenarios. */

import { PlusIcon } from "@heroicons/react/16/solid";
import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import {
  createScenarioOnDisk,
  deleteScenario,
  enqueueRuns,
  openScenarioFolder,
  projectHasNetwork,
  projectResultsSizes,
  renameScenario,
  useScenarios,
} from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import { formatBytes } from "../../units";
import { DeleteConfirmModal } from "../modals/DeleteConfirmModal";
import { BaseRow, CreateRow, ScenarioRow } from "./ScenariosPanel/Rows";
import {
  descendants,
  directChildren,
  type FlatScenario,
  flattenScenarios,
} from "./ScenariosPanel/shared";

// ── Main component ───────────────────────────────────────────────────────────

export function ScenariosPanel({
  showHeader = true,
}: {
  showHeader?: boolean;
}) {
  const { project, accent } = useActiveProject();
  // A scenario is a variant of the base model, so an empty project has
  // nothing to branch. The backend refuses too — this only keeps the UI from
  // offering an action that can only fail.
  const hasNetwork = projectHasNetwork(project);
  const {
    showToast,
    activeScenarioId,
    setActiveScenarioId,
    scenariosVersion,
    bumpScenarios,
    requestClearResults,
    openTaskTray,
  } = useAppState();

  const rawDtos = useScenarios(project?.id ?? null, scenariosVersion);
  const scenarios = useMemo(() => flattenScenarios(rawDtos), [rawDtos]);

  // How many targets across the project hold results, base model included.
  // Drives whether a project-wide clear is worth offering at all.
  const simulatedCount = useMemo(
    () =>
      (project?.state === "simulated" ? 1 : 0) +
      scenarios.filter((s) => s.state === "simulated").length,
    [project?.state, scenarios],
  );

  // If active scenario was deleted, fall back to Base.
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

  // ── per-row state ────────────────────────────────────────────────────────
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [deletingId, setDeletingId] = useState<string | null>(null);
  /** Scenario awaiting delete confirmation (trash click → confirm → delete). */
  const [pendingDelete, setPendingDelete] = useState<FlatScenario | null>(null);
  /** Opt-in on the delete prompt. Reset every time it opens — a remembered
   * checkbox on a destructive dialog is how someone deletes a whole branch by
   * accident. */
  const [deleteCascade, setDeleteCascade] = useState(false);
  const [runningId, setRunningId] = useState<string | null>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (renamingId) setTimeout(() => renameInputRef.current?.focus(), 0);
  }, [renamingId]);

  // ── create new scenario ──────────────────────────────────────────────────
  const [creating, setCreating] = useState(false);
  const [createName, setCreateName] = useState("");
  const [createParentId, setCreateParentId] = useState<string | null>(null);
  const createInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (creating) setTimeout(() => createInputRef.current?.focus(), 0);
  }, [creating]);

  const handleCreate = useCallback(async () => {
    const name = createName.trim();
    if (!name || !project) {
      setCreating(false);
      setCreateName("");
      return;
    }
    const result = await createScenarioOnDisk({
      projectId: project.id,
      name,
      parentScenarioId: createParentId,
    });
    setCreating(false);
    setCreateName("");
    setCreateParentId(null);
    if (result) {
      bumpScenarios();
      showToast(`"${result.name}" created`, "success");
    } else {
      showToast("Failed to create scenario", "error");
    }
  }, [createName, createParentId, project, bumpScenarios, showToast]);

  // One call for every target in the project — each row menu labels its clear
  // with what it reclaims, and a per-row command would cost a round trip
  // each. Refetched on `scenariosVersion`, which bumps when a run finishes or
  // results are cleared.
  const [sizes, setSizes] = useState<{
    base: number;
    scenarios: Record<string, number>;
    total: number;
  } | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `scenariosVersion` is not read here — it is the intentional refetch trigger, bumped when a run finishes or results are cleared.
  useEffect(() => {
    const id = project?.id;
    if (!id) {
      setSizes(null);
      return;
    }
    let cancelled = false;
    void projectResultsSizes(id).then((next) => {
      if (!cancelled) setSizes(next);
    });
    return () => {
      cancelled = true;
    };
  }, [project?.id, scenariosVersion]);

  /** "Frees 12.4 MB", or nothing while unmeasured or with nothing to free. */
  const freed = (bytes: number | undefined) =>
    bytes ? `Frees ${formatBytes(bytes)}` : undefined;

  // Two different counts, because they answer two different questions.
  // Everything beneath the doomed scenario *survives* — each is a full copy
  // of its parent's model, not a delta — so that total is the reassuring
  // one. Only the direct children visibly *move*: `buildScenarioTree`
  // promotes them to roots once their parent id stops resolving, while a
  // grandchild still resolves its own parent and keeps its place.
  const survivingCount = pendingDelete
    ? descendants(rawDtos, pendingDelete.id).length
    : 0;
  const promotedCount = pendingDelete
    ? directChildren(rawDtos, pendingDelete.id).length
    : 0;

  // ── handlers ─────────────────────────────────────────────────────────────

  const handleRenameCommit = useCallback(
    async (s: FlatScenario) => {
      const name = renameValue.trim();
      setRenamingId(null);
      setRenameValue("");
      if (!name || name === s.name || !project) return;
      const ok = await renameScenario(project.id, s.id, name);
      if (ok) {
        bumpScenarios();
      } else {
        showToast("Rename failed", "error");
      }
    },
    [renameValue, project, bumpScenarios, showToast],
  );

  const handleDelete = useCallback(
    async (s: FlatScenario, cascade: boolean) => {
      if (!project) return;
      setDeletingId(s.id);
      const removed = await deleteScenario(project.id, s.id, cascade);
      setDeletingId(null);
      if (removed === 0) {
        showToast("Delete failed", "error");
        return;
      }
      bumpScenarios();
      // The active scenario may have been a descendant rather than the one
      // clicked, so fall back to Base whenever it no longer exists.
      if (
        activeScenarioId === s.id ||
        (cascade &&
          activeScenarioId != null &&
          descendants(rawDtos, s.id).some((d) => d.id === activeScenarioId))
      ) {
        setActiveScenarioId(null);
      }
      showToast(
        removed === 1
          ? `"${s.name}" deleted`
          : `"${s.name}" and ${removed - 1} branched scenario${removed === 2 ? "" : "s"} deleted`,
        "info",
      );
    },
    [
      project,
      activeScenarioId,
      rawDtos,
      bumpScenarios,
      setActiveScenarioId,
      showToast,
    ],
  );

  const handleRun = useCallback(
    async (s: FlatScenario) => {
      if (!project) return;
      setRunningId(s.id);
      try {
        await enqueueRuns(project.id, [s.id]);
        bumpScenarios();
        openTaskTray();
      } catch (err) {
        showToast(`Failed to queue run: ${formatIpcError(err)}`, "error");
      } finally {
        setRunningId(null);
      }
    },
    [project, bumpScenarios, openTaskTray, showToast],
  );

  const handleActivate = useCallback(
    (s: FlatScenario) => {
      setActiveScenarioId(s.id);
    },
    [setActiveScenarioId],
  );

  const handleOpenFolder = useCallback(
    async (s: FlatScenario) => {
      if (!project) return;
      await openScenarioFolder(project.id, s.id);
    },
    [project],
  );

  const handleBranch = useCallback((s: FlatScenario) => {
    setCreateParentId(s.id);
    setCreating(true);
  }, []);

  // ── render ────────────────────────────────────────────────────────────────

  if (!project) return null;

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
        animation: "fadeIn 150ms ease-out",
        fontFamily: "var(--font-ui)",
      }}
    >
      {/* Header toolbar */}
      {showHeader && (
        <div
          style={{
            flexShrink: 0,
            height: 52,
            borderBottom: "1px solid var(--border)",
            background: "var(--bg-panel)",
            display: "flex",
            alignItems: "center",
            padding: "0 20px",
            gap: 12,
          }}
        >
          <span
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            Scenarios
          </span>
          <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            {scenarios.length} scenario{scenarios.length !== 1 ? "s" : ""}
          </span>

          <div style={{ flex: 1 }} />

          <button
            type="button"
            onClick={() => {
              setCreateParentId(null);
              setCreating(true);
            }}
            disabled={!hasNetwork}
            data-tooltip={
              hasNetwork ? undefined : "This project has no network to branch"
            }
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
              padding: "5px 12px",
              border: `1px solid ${accent}`,
              borderRadius: 6,
              background: `${accent}22`,
              color: accent,
              fontSize: 12,
              fontWeight: 500,
              cursor: hasNetwork ? "pointer" : "default",
              opacity: hasNetwork ? 1 : 0.45,
              fontFamily: "var(--font-ui)",
            }}
          >
            <PlusIcon style={{ width: 12, height: 12 }} />
            New scenario
          </button>
        </div>
      )}

      {/* Scrollable content */}
      <div style={{ flex: 1, overflow: "auto", padding: "20px" }}>
        <div
          style={{
            background: "var(--bg-panel)",
            border: "1px solid var(--border)",
            borderRadius: 8,
            overflow: "hidden",
          }}
        >
          {/* Base model row — always first */}
          <BaseRow
            isActive={activeScenarioId === null}
            accent={accent}
            onActivate={() => setActiveScenarioId(null)}
            onNewScenario={() => {
              setCreateParentId(null);
              setCreating(true);
            }}
            canBranch={hasNetwork}
            simulated={project?.state === "simulated"}
            clearDetail={freed(sizes?.base)}
            clearAllDetail={freed(sizes?.total)}
            onClearResults={() =>
              project &&
              requestClearResults({
                projectId: project.id,
                scope: "target",
                scenarioId: null,
                name: "Base model",
              })
            }
            clearAllCount={simulatedCount}
            onClearAllResults={() =>
              project &&
              requestClearResults({
                projectId: project.id,
                scope: "all",
                scenarioId: null,
                name: project.name,
                simulatedCount,
              })
            }
          />

          {/* Inline create row (when branching from base) */}
          {creating && createParentId === null && (
            <CreateRow
              ref={createInputRef}
              value={createName}
              parentName={null}
              onChange={setCreateName}
              onCommit={handleCreate}
              onCancel={() => {
                setCreating(false);
                setCreateName("");
              }}
            />
          )}

          {scenarios.length === 0 && !creating && (
            <div
              style={{
                padding: "32px 20px",
                textAlign: "center",
                color: "var(--text-tertiary)",
                fontSize: 13,
                borderTop: "1px solid var(--border)",
              }}
            >
              No named scenarios yet.{" "}
              {!hasNetwork ? (
                "Import or build a network first."
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    setCreateParentId(null);
                    setCreating(true);
                  }}
                  style={{
                    background: "none",
                    border: "none",
                    color: accent,
                    cursor: "pointer",
                    fontSize: 13,
                    padding: 0,
                    fontFamily: "var(--font-ui)",
                    textDecoration: "underline",
                    textUnderlineOffset: 2,
                  }}
                >
                  Create one from the base model.
                </button>
              )}
            </div>
          )}

          {scenarios.map((s) => (
            <React.Fragment key={s.id}>
              <ScenarioRow
                scenario={s}
                isActive={s.id === activeScenarioId}
                accent={accent}
                isRenaming={renamingId === s.id}
                renameValue={renameValue}
                renameInputRef={
                  renamingId === s.id ? renameInputRef : undefined
                }
                isDeleting={deletingId === s.id}
                isRunning={runningId === s.id}
                parentName={
                  s.parentScenarioId
                    ? (scenarios.find((p) => p.id === s.parentScenarioId)
                        ?.name ?? s.parentScenarioId)
                    : null
                }
                onActivate={() => handleActivate(s)}
                onRenameStart={() => {
                  setRenamingId(s.id);
                  setRenameValue(s.name);
                }}
                onRenameChange={setRenameValue}
                onRenameCommit={() => handleRenameCommit(s)}
                onRenameCancel={() => {
                  setRenamingId(null);
                  setRenameValue("");
                }}
                onBranch={() => handleBranch(s)}
                clearDetail={freed(sizes?.scenarios[s.id])}
                onClearResults={() =>
                  project &&
                  requestClearResults({
                    projectId: project.id,
                    scope: "target",
                    scenarioId: s.id,
                    name: s.name,
                  })
                }
                onRun={() => handleRun(s)}
                onDelete={() => {
                  setDeleteCascade(false);
                  setPendingDelete(s);
                }}
                onOpenFolder={() => handleOpenFolder(s)}
              />

              {/* Inline create row for branching from this scenario */}
              {creating && createParentId === s.id && (
                <CreateRow
                  ref={createInputRef}
                  value={createName}
                  parentName={s.name}
                  onChange={setCreateName}
                  onCommit={handleCreate}
                  onCancel={() => {
                    setCreating(false);
                    setCreateName("");
                    setCreateParentId(null);
                  }}
                  indent={s.depth + 1}
                />
              )}
            </React.Fragment>
          ))}
        </div>
      </div>

      {/* Delete confirmation — deleting a scenario destroys its INP and
          simulation results, so it must never be a single-click action. */}
      <DeleteConfirmModal
        open={!!pendingDelete}
        elementKind="scenario"
        elementId={pendingDelete?.name ?? ""}
        message={
          <>
            Delete scenario{" "}
            <strong style={{ color: "var(--text-primary)" }}>
              {pendingDelete?.name}
            </strong>
            ? Its network changes and simulation results will be permanently
            removed.
            {survivingCount > 0 &&
              (deleteCascade ? (
                <>
                  {" "}
                  All {survivingCount} scenario
                  {survivingCount === 1 ? "" : "s"} branched from it will be
                  deleted too, with their own networks and results.
                </>
              ) : (
                <>
                  {" "}
                  {survivingCount} scenario{survivingCount === 1 ? "" : "s"}{" "}
                  branched from it {survivingCount === 1 ? "is" : "are"}{" "}
                  untouched — each is a complete model of its own.{" "}
                  {survivingCount === promotedCount ? (
                    <>
                      {promotedCount === 1 ? "It" : "They"} will move to the top
                      level.
                    </>
                  ) : (
                    <>
                      The {promotedCount} branched directly from it will move to
                      the top level.
                    </>
                  )}
                </>
              ))}
          </>
        }
        option={
          survivingCount > 0
            ? {
                label: `Also delete the ${survivingCount} scenario${survivingCount === 1 ? "" : "s"} branched from it`,
                checked: deleteCascade,
                onChange: setDeleteCascade,
              }
            : undefined
        }
        onCancel={() => setPendingDelete(null)}
        onConfirm={() => {
          const s = pendingDelete;
          const cascade = deleteCascade;
          setPendingDelete(null);
          if (s) void handleDelete(s, cascade);
        }}
      />
    </div>
  );
}
