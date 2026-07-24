import {
  ChevronUpDownIcon,
  MagnifyingGlassIcon,
  PencilIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  getDraftDirtyCount,
  saveDraftsViaGuard,
  useActiveProject,
  useAppState,
} from "../../AppContext";
import { type Project, renameProjectOnDisk, useProjects } from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

// ─────────────────────────────────────────────────────────────────────────────
// ProjectSwitcher — the breadcrumb's project segment.
//
// One control that both switches between projects and renames the current one,
// without leaving for the Projects page. The two text-entry purposes are kept
// physically separate (a filter field for switching, an inline field for
// renaming) so neither is ambiguous.
//
//   [ Project name ⌄ ]   ← collapsed trigger
//   ┌─────────────────────────────┐
//   │  Current name          ✎    │  ← rename in place (samples no-op w/ toast)
//   ├─────────────────────────────┤
//   │  🔍 Filter…                 │  ← only when there are many others
//   │  Other project A            │  ← click → switch (guarded if edits pending)
//   │  Other project B            │
//   └─────────────────────────────┘
// ─────────────────────────────────────────────────────────────────────────────

/** Show the filter field once the other-project list gets long enough to scan. */
const FILTER_THRESHOLD = 8;
/** Hard cap on rendered rows so a huge library can't flood the DOM. */
const MAX_ROWS = 100;

export function ProjectSwitcher() {
  const {
    activeProjectId,
    openProject,
    projectView,
    bumpProjects,
    showToast,
    projectsVersion,
  } = useAppState();
  const { project } = useActiveProject();
  const projects = useProjects(projectsVersion);

  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [draftName, setDraftName] = useState("");
  // Project id awaiting the unsaved-edits guard, with the dirty count captured
  // at the moment of the switch attempt so the message stays stable.
  const [pendingSwitch, setPendingSwitch] = useState<{
    id: string;
    name: string;
    dirtyCount: number;
  } | null>(null);

  const rootRef = useRef<HTMLDivElement | null>(null);
  const renameInputRef = useRef<HTMLInputElement | null>(null);
  const filterInputRef = useRef<HTMLInputElement | null>(null);

  const projectName = project?.name ?? "";

  // Close the dropdown on outside click.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
        setRenaming(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // Focus the right field when entering rename / opening the dropdown.
  useEffect(() => {
    if (renaming) {
      renameInputRef.current?.focus();
      renameInputRef.current?.select();
    }
  }, [renaming]);

  const others = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return projects
      .filter((p) => p.id !== activeProjectId)
      .filter((p) => (q ? p.name.toLowerCase().includes(q) : true))
      .sort(
        (a, b) =>
          (b.modifiedAtMs ?? 0) - (a.modifiedAtMs ?? 0) ||
          a.name.localeCompare(b.name),
      );
  }, [projects, activeProjectId, filter]);

  const showFilter =
    projects.filter((p) => p.id !== activeProjectId).length >= FILTER_THRESHOLD;
  const shown = others.slice(0, MAX_ROWS);
  const overflow = others.length - shown.length;

  if (!project) return null;

  // ── Rename ────────────────────────────────────────────────────────────────
  function startRename() {
    setDraftName(projectName);
    setRenaming(true);
  }

  async function commitRename() {
    const next = draftName.trim();
    setRenaming(false);
    if (!project || !next || next === projectName) return;
    const updated = await renameProjectOnDisk(project.id, next);
    if (updated) {
      bumpProjects();
      showToast(`Renamed to "${next}"`, "success");
    } else {
      showToast("Cannot rename a built-in sample project", "warn");
    }
  }

  // ── Switch (guarded) ────────────────────────────────────────────────────────
  function attemptSwitch(target: Project) {
    if (target.id === activeProjectId) {
      closeDropdown();
      return;
    }
    const dirty = getDraftDirtyCount();
    if (dirty > 0) {
      setPendingSwitch({ id: target.id, name: target.name, dirtyCount: dirty });
      return;
    }
    doSwitch(target.id);
  }

  function doSwitch(id: string) {
    // Preserve the current tab across the switch (P1 Canvas -> P2 Canvas)
    // rather than resuming the target's last-active tab.
    openProject(id, projectView);
    closeDropdown();
  }

  function closeDropdown() {
    setOpen(false);
    setRenaming(false);
    setFilter("");
    setPendingSwitch(null);
  }

  async function saveThenSwitch() {
    const pending = pendingSwitch;
    if (!pending) return;
    const result = await saveDraftsViaGuard();
    if (result && result.failed > 0) {
      showToast(
        `Could not save all changes: ${result.errors[0] ? formatIpcError(result.errors[0]) : `${result.failed} failed`}`,
        "error",
      );
      setPendingSwitch(null);
      return;
    }
    doSwitch(pending.id);
  }

  return (
    <div ref={rootRef} style={{ position: "relative", display: "inline-flex" }}>
      {/* Collapsed trigger */}
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        data-tooltip="Switch or rename project"
        data-tooltip-pos="bottom"
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 3,
          border: "1px solid transparent",
          background: open ? "var(--bg-card)" : "transparent",
          color: "var(--text-secondary)",
          cursor: "pointer",
          fontSize: 13,
          fontWeight: 400,
          fontFamily: "var(--font-ui)",
          padding: "2px 4px 2px 6px",
          borderRadius: 5,
          transition: "background var(--t-fast), color var(--t-fast)",
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.color = "var(--text-primary)";
          if (!open) e.currentTarget.style.background = "var(--bg-card)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.color = "var(--text-secondary)";
          if (!open) e.currentTarget.style.background = "transparent";
        }}
      >
        {projectName}
        <ChevronUpDownIcon
          style={{ width: 13, height: 13, color: "var(--text-disabled)" }}
        />
      </button>

      {/* Dropdown */}
      {open && (
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            left: 0,
            minWidth: 260,
            maxWidth: 360,
            background: "var(--bg-panel)",
            border: "1px solid var(--border-hover)",
            borderRadius: 8,
            boxShadow: "var(--shadow-3)",
            zIndex: 150,
            overflow: "hidden",
            fontFamily: "var(--font-ui)",
          }}
        >
          {/* Current project + rename */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "8px 10px",
              borderBottom: "1px solid var(--border)",
            }}
          >
            <span
              style={{
                fontSize: 10,
                fontWeight: 700,
                letterSpacing: "0.06em",
                textTransform: "uppercase",
                color: "var(--text-tertiary)",
                flexShrink: 0,
              }}
            >
              Current
            </span>
            {renaming ? (
              <input
                ref={renameInputRef}
                value={draftName}
                onChange={(e) => setDraftName(e.target.value)}
                onBlur={commitRename}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitRename();
                  else if (e.key === "Escape") {
                    setRenaming(false);
                    setDraftName(projectName);
                  }
                }}
                style={{
                  flex: 1,
                  minWidth: 0,
                  background: "var(--bg-input)",
                  border: "1px solid var(--accent)",
                  borderRadius: 4,
                  color: "var(--text-primary)",
                  font: "inherit",
                  fontSize: 13,
                  padding: "2px 6px",
                  outline: "none",
                }}
              />
            ) : (
              <>
                <span
                  style={{
                    flex: 1,
                    minWidth: 0,
                    fontSize: 13,
                    fontWeight: 600,
                    color: "var(--text-primary)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {projectName}
                </span>
                <button
                  type="button"
                  onClick={startRename}
                  data-tooltip="Rename project"
                  data-tooltip-pos="bottom"
                  style={{
                    flexShrink: 0,
                    border: "none",
                    background: "transparent",
                    color: "var(--text-disabled)",
                    cursor: "pointer",
                    padding: "2px 3px",
                    borderRadius: 4,
                    display: "inline-flex",
                    alignItems: "center",
                    lineHeight: 1,
                    transition: "color var(--t-fast)",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.color = "var(--text-secondary)";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.color = "var(--text-disabled)";
                  }}
                >
                  <PencilIcon style={{ width: 12, height: 12 }} />
                </button>
              </>
            )}
          </div>

          {/* Filter (only when there are many others) */}
          {showFilter && (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "6px 10px",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <MagnifyingGlassIcon
                style={{ width: 13, height: 13, color: "var(--text-disabled)" }}
              />
              <input
                ref={filterInputRef}
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && shown[0]) attemptSwitch(shown[0]);
                  else if (e.key === "Escape") setOpen(false);
                }}
                placeholder="Filter projects…"
                // biome-ignore lint/a11y/noAutofocus: focus belongs on the filter when the switcher opens.
                autoFocus
                style={{
                  flex: 1,
                  minWidth: 0,
                  background: "transparent",
                  border: "none",
                  color: "var(--text-primary)",
                  font: "inherit",
                  fontSize: 12,
                  outline: "none",
                }}
              />
            </div>
          )}

          {/* Other projects */}
          <div style={{ maxHeight: 300, overflowY: "auto", padding: "4px 0" }}>
            {shown.length === 0 ? (
              <div
                style={{
                  padding: "10px 12px",
                  fontSize: 12,
                  color: "var(--text-tertiary)",
                }}
              >
                {filter.trim() ? "No matching projects" : "No other projects"}
              </div>
            ) : (
              shown.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => attemptSwitch(p)}
                  style={{
                    width: "100%",
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    padding: "7px 12px",
                    border: "none",
                    background: "transparent",
                    color: "var(--text-primary)",
                    cursor: "pointer",
                    fontSize: 13,
                    fontFamily: "var(--font-ui)",
                    textAlign: "left",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "var(--nav-hover)";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "transparent";
                  }}
                >
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {p.name}
                  </span>
                  <span
                    style={{
                      fontSize: 11,
                      color: "var(--text-disabled)",
                      flexShrink: 0,
                    }}
                  >
                    {p.modifiedLabel}
                  </span>
                </button>
              ))
            )}
            {overflow > 0 && (
              <div
                style={{
                  padding: "6px 12px",
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                }}
              >
                +{overflow} more — filter to narrow
              </div>
            )}
          </div>
        </div>
      )}

      {/* Unsaved-edits guard */}
      {pendingSwitch && (
        <ModalBackdrop
          onDismiss={() => setPendingSwitch(null)}
          zIndex={300}
          style={{ animation: "fadeIn 120ms ease-out" }}
        >
          <div
            {...stopBackdropEvents}
            style={{
              width: "100%",
              maxWidth: 420,
              background: "var(--bg-panel)",
              border: "1px solid var(--border-hover)",
              borderRadius: 12,
              boxShadow: "var(--shadow-3)",
              overflow: "hidden",
              fontFamily: "var(--font-ui)",
              animation: "scaleIn 160ms ease-out",
            }}
          >
            <div style={{ padding: "18px 20px" }}>
              <div
                style={{
                  fontSize: 14,
                  fontWeight: 600,
                  color: "var(--text-primary)",
                  marginBottom: 6,
                }}
              >
                Unsaved changes
              </div>
              <div
                style={{
                  fontSize: 13,
                  color: "var(--text-secondary)",
                  lineHeight: 1.5,
                }}
              >
                You have {pendingSwitch.dirtyCount} unsaved editor change
                {pendingSwitch.dirtyCount !== 1 ? "s" : ""}. Switching to{" "}
                <strong style={{ color: "var(--text-primary)" }}>
                  {pendingSwitch.name}
                </strong>{" "}
                will discard them unless you save first.
              </div>
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: "flex-end",
                gap: 10,
                padding: "12px 20px",
                borderTop: "1px solid var(--border)",
                background: "rgba(0,0,0,0.18)",
              }}
            >
              <button
                type="button"
                onClick={() => setPendingSwitch(null)}
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
                type="button"
                onClick={() => {
                  const id = pendingSwitch.id;
                  setPendingSwitch(null);
                  doSwitch(id);
                }}
                style={{
                  background: "transparent",
                  border: "1px solid var(--border)",
                  color: "var(--status-error, #ef4444)",
                  borderRadius: 5,
                  padding: "7px 14px",
                  fontSize: 12,
                  cursor: "pointer",
                  fontFamily: "var(--font-ui)",
                }}
              >
                Discard & switch
              </button>
              <button
                type="button"
                onClick={saveThenSwitch}
                style={{
                  background: "var(--accent)",
                  border: "1px solid var(--accent)",
                  color: "#fff",
                  borderRadius: 5,
                  padding: "7px 14px",
                  fontSize: 12,
                  fontWeight: 600,
                  cursor: "pointer",
                  fontFamily: "var(--font-ui)",
                }}
              >
                Save & switch
              </button>
            </div>
          </div>
        </ModalBackdrop>
      )}
    </div>
  );
}
