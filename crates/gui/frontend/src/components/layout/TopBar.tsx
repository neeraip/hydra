import { ArrowLeftIcon, ArrowRightIcon } from "@heroicons/react/24/outline";
import { useActiveProject, useAppState } from "../../AppContext";
import { PROJECT_VIEWS } from "../../hooks";
import { ProjectSwitcher } from "./ProjectSwitcher";

// ── TopBar ────────────────────────────────────────────────────────────────────
//
// Global navigation bar rendered above all pages. On the project page it
// shows a full breadcrumb (Projects / [project switcher] / [view]).
// On all other pages it shows the plain page name.
//
// The project segment is the ProjectSwitcher (switch + rename); the Run button
// lives in ProjectToolbar — TopBar is purely navigational.

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
    canNavBack,
    canNavForward,
    navBack,
    navForward,
  } = useAppState();
  const { project } = useActiveProject();

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
        // ── Project breadcrumb: Projects / [switcher] / [view] ────────────
        <>
          <button
            type="button"
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

          <ProjectSwitcher />

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
      type="button"
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
