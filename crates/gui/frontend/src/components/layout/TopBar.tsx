import {
  ArrowLeftIcon,
  ArrowRightIcon,
  PencilIcon,
} from "@heroicons/react/24/outline";
import { useEffect, useRef, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { PROJECT_VIEWS, renameProjectOnDisk } from "../../hooks";

// ── TopBar ────────────────────────────────────────────────────────────────────
//
// Global navigation bar rendered above all pages. On the project page it
// shows a full breadcrumb (Projects / [project name] / [view]).
// On all other pages it shows the plain page name.
//
// The Run button lives in ScenarioStrip — TopBar is purely navigational.

const PAGE_LABELS: Partial<Record<string, string>> = {
  home: "Home",
  projects: "Projects",
  settings: "Settings",
};

export function TopBar() {
  const {
    page,
    projectView,
    closeProject,
    bumpProjects,
    showToast,
    canNavBack,
    canNavForward,
    navBack,
    navForward,
  } = useAppState();
  const { project } = useActiveProject();

  const [renaming, setRenaming] = useState(false);
  const [draftName, setDraftName] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  const projectName = project?.name ?? "";

  useEffect(() => {
    if (renaming) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [renaming]);

  // Keep draft in sync when the project name changes externally.
  useEffect(() => {
    if (!renaming) setDraftName(projectName);
  }, [projectName, renaming]);

  const startRename = () => {
    if (!project) return;
    setDraftName(projectName);
    setRenaming(true);
  };

  const commitRename = async () => {
    if (!project) {
      setRenaming(false);
      return;
    }
    const next = draftName.trim();
    setRenaming(false);
    if (!next || next === projectName) return;
    const updated = await renameProjectOnDisk(project.id, next);
    if (updated) {
      bumpProjects();
      showToast(`Renamed to "${next}"`, "success");
    } else {
      showToast("Cannot rename a built-in sample project", "warn");
    }
  };

  const views = project ? PROJECT_VIEWS : [];
  const viewSpec = views.find((v) => v.id === projectView);
  const viewLabel = viewSpec?.label ?? null;

  return (
    <div
      style={{
        height: 44,
        background: "var(--bg-panel)",
        borderBottom: "1px solid var(--border)",
        display: "flex",
        alignItems: "center",
        padding: "0 16px",
        gap: 10,
        flexShrink: 0,
      }}
    >
      {/* Back / Forward nav arrows */}
      <NavArrowButton title="Back" onClick={navBack} disabled={!canNavBack}>
        <ArrowLeftIcon style={{ width: 14, height: 14 }} />
      </NavArrowButton>
      <NavArrowButton
        title="Forward"
        onClick={navForward}
        disabled={!canNavForward}
      >
        <ArrowRightIcon style={{ width: 14, height: 14 }} />
      </NavArrowButton>

      <div
        style={{
          width: 1,
          height: 18,
          background: "var(--border)",
          flexShrink: 0,
        }}
      />

      {page === "project" ? (
        // ── Project breadcrumb: Projects / [engine] / [name] / [view] ─────
        <>
          <button
            onClick={closeProject}
            style={{
              border: "none",
              background: "transparent",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              fontSize: 13,
              fontFamily: "var(--font-ui)",
              padding: "2px 4px",
              borderRadius: 4,
              transition: "color var(--t-fast)",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-tertiary)";
            }}
          >
            Projects
          </button>
          <span style={{ color: "var(--text-disabled)", fontSize: 13 }}>/</span>

          {renaming ? (
            <input
              ref={inputRef}
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
                background: "var(--bg-input)",
                border: "1px solid var(--accent)",
                borderRadius: 4,
                color: "var(--text-primary)",
                font: "inherit",
                fontSize: 13,
                fontWeight: viewLabel ? 400 : 500,
                padding: "2px 6px",
                outline: "none",
                minWidth: 120,
              }}
            />
          ) : (
            <>
              <span
                style={{
                  color: viewLabel
                    ? "var(--text-secondary)"
                    : "var(--text-primary)",
                  fontSize: 13,
                  fontWeight: viewLabel ? 400 : 500,
                }}
              >
                {projectName}
              </span>
              {project && (
                <button
                  onClick={startRename}
                  data-tooltip="Rename project"
                  data-tooltip-pos="bottom"
                  style={{
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
                    (e.currentTarget as HTMLButtonElement).style.color =
                      "var(--text-secondary)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLButtonElement).style.color =
                      "var(--text-disabled)";
                  }}
                >
                  <PencilIcon style={{ width: 12, height: 12 }} />
                </button>
              )}
            </>
          )}

          {viewLabel && (
            <>
              <span style={{ color: "var(--text-disabled)", fontSize: 13 }}>
                /
              </span>
              <span
                style={{
                  color: "var(--text-primary)",
                  fontSize: 13,
                  fontWeight: 500,
                }}
              >
                {viewLabel}
              </span>
            </>
          )}
        </>
      ) : (
        // ── Plain page label ───────────────────────────────────────────────
        <span
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
          }}
        >
          {PAGE_LABELS[page] ?? page}
        </span>
      )}

      <div style={{ flex: 1 }} />
    </div>
  );
}

function NavArrowButton({
  title,
  onClick,
  disabled,
  children,
}: {
  title: string;
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      data-tooltip={title}
      data-tooltip-pos="bottom"
      onClick={onClick}
      disabled={disabled}
      style={{
        width: 28,
        height: 28,
        borderRadius: 5,
        background: "transparent",
        border: "1px solid transparent",
        color: disabled ? "var(--text-disabled)" : "var(--text-secondary)",
        cursor: disabled ? "not-allowed" : "pointer",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "background var(--t-fast), border-color var(--t-fast)",
      }}
      onMouseEnter={(e) => {
        if (!disabled)
          (e.currentTarget as HTMLButtonElement).style.background =
            "var(--bg-card)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = "transparent";
      }}
    >
      {children}
    </button>
  );
}
