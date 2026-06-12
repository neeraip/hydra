/* Scenario management page — create, rename, branch, run, and delete scenarios. */

import { PlusIcon } from "@heroicons/react/16/solid";
/* Scenario management page — create, rename, branch, run, and delete scenarios. */
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
  renameScenario,
  useScenarios,
} from "../../hooks";
import { BaseRow, CreateRow, ScenarioRow } from "./ScenariosPanel/Rows";
import { type FlatScenario, flattenScenarios } from "./ScenariosPanel/shared";

// ── Main component ───────────────────────────────────────────────────────────

export function ScenariosPanel({
  showHeader = true,
}: {
  showHeader?: boolean;
}) {
  const { project, accent } = useActiveProject();
  const {
    showToast,
    activeScenarioId,
    setActiveScenarioId,
    scenariosVersion,
    bumpScenarios,
    openTaskTray,
  } = useAppState();

  const rawDtos = useScenarios(project?.id ?? null, scenariosVersion);
  const scenarios = useMemo(() => flattenScenarios(rawDtos), [rawDtos]);

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
    async (s: FlatScenario) => {
      if (!project) return;
      setDeletingId(s.id);
      const ok = await deleteScenario(project.id, s.id);
      setDeletingId(null);
      if (ok) {
        bumpScenarios();
        if (activeScenarioId === s.id) setActiveScenarioId(null);
        showToast(`"${s.name}" deleted`, "info");
      } else {
        showToast("Delete failed", "error");
      }
    },
    [project, activeScenarioId, bumpScenarios, setActiveScenarioId, showToast],
  );

  const handleRun = useCallback(
    async (s: FlatScenario) => {
      if (!project) return;
      setRunningId(s.id);
      await enqueueRuns(project.id, [s.id]);
      setRunningId(null);
      bumpScenarios();
      openTaskTray();
    },
    [project, bumpScenarios, openTaskTray],
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
              cursor: "pointer",
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
                onRun={() => handleRun(s)}
                onDelete={() => handleDelete(s)}
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
    </div>
  );
}
