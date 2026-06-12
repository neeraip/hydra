import {
  BoltIcon,
  Cog6ToothIcon,
  FolderIcon,
} from "@heroicons/react/24/outline";
import { useActiveProject, useAppState, useTasks } from "../../AppContext";
import { PROJECT_VIEWS } from "../../hooks";
import { NavButton } from "../ui/NavButton";

const ICON = { width: 20, height: 20 };

export function ActivityBar() {
  const {
    page,
    projectView,
    setPage,
    setProjectView,
    openCommandPalette,
    toggleTaskTray,
    taskTrayOpen,
    closeProject,
    activeProjectId,
  } = useAppState();
  const { project } = useActiveProject();
  const tasks = useTasks();

  const runningCount = tasks.filter((t) => t.status === "running").length;
  const failedCount = tasks.filter((t) => t.status === "failed").length;
  const hasActivity = runningCount > 0 || failedCount > 0;

  function handleHomeClick() {
    if (activeProjectId) {
      closeProject();
    } else {
      setPage("home");
    }
  }

  return (
    <div
      style={{
        width: "var(--activity-w)",
        height: "100%",
        background: "var(--bg-activity)",
        borderRight: "1px solid var(--border)",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        paddingTop: 8,
        paddingBottom: 8,
        gap: 2,
        flexShrink: 0,
        zIndex: 30,
      }}
    >
      {/* ── Logo / Home ────────────────────────────────────────────────────── */}
      <button
        type="button"
        onClick={handleHomeClick}
        aria-label="Home"
        data-tooltip="Home"
        data-tooltip-pos="right"
        className="logo-btn"
        style={{
          width: 36,
          height: 36,
          marginBottom: 8,
          border: "none",
          borderRadius: 9,
          background: "var(--accent)",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
          padding: 0,
        }}
      >
        {/* Hydra wordmark glyph */}
        <svg
          width="18"
          height="18"
          viewBox="0 0 18 18"
          fill="none"
          aria-hidden="true"
        >
          <path
            d="M3 3v12M3 9h12M15 3v12"
            stroke="#fff"
            strokeWidth="2.2"
            strokeLinecap="round"
          />
        </svg>
      </button>

      {/* ── Global nav ─────────────────────────────────────────────────────── */}
      <div className="divider" />
      <NavButton
        icon={<FolderIcon {...ICON} />}
        label="Projects"
        active={page === "projects"}
        onClick={() =>
          page === "project" ? setPage("projects") : setPage("projects")
        }
      />

      {/* ── Project sub-nav (child items under Projects) ────────────────────
           box-shadow used for top/bottom lines so no layout shift occurs.   */}
      {page === "project" &&
        activeProjectId &&
        (() => {
          const proj = project;
          if (!proj) return null;
          const views = PROJECT_VIEWS;
          return (
            <div
              style={{
                width: "100%",
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                gap: 2,
                paddingTop: 4,
                paddingBottom: 4,
                background: "var(--bg-app)",
                boxShadow: "0 -1px 0 var(--border), 0 1px 0 var(--border)",
              }}
            >
              {views.map(({ id, label, icon: Icon }) => (
                <NavButton
                  key={id}
                  icon={<Icon width={18} height={18} />}
                  label={label}
                  active={projectView === id}
                  onClick={() => setProjectView(id)}
                />
              ))}
            </div>
          );
        })()}

      {/* ── Command palette hint ────────────────────────────────────────────── */}
      <div style={{ flex: 1 }} />

      {/* ⌘K opens the command palette; ? opens keyboard shortcuts */}
      <button
        type="button"
        onClick={openCommandPalette}
        data-tooltip="Command Palette (⌘K) · Shortcuts (?)"
        data-tooltip-pos="right"
        aria-label="Command Palette"
        className="cmd-palette-btn"
        style={{
          width: 32,
          height: 32,
          border: "1px solid var(--border-hover)",
          borderRadius: 7,
          background: "var(--bg-input)",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          marginBottom: 4,
          flexShrink: 0,
          fontSize: 11,
          fontFamily: "var(--font-mono)",
        }}
      >
        ⌘K
      </button>

      {/* ── Task monitor ───────────────────────────────────────────────────── */}
      <NavButton
        icon={<BoltIcon {...ICON} />}
        label={`Task Monitor${hasActivity ? " — tasks in progress" : ""}`}
        active={taskTrayOpen}
        badgeCount={failedCount > 0 ? failedCount : undefined}
        pulse={runningCount > 0 && failedCount === 0}
        onClick={toggleTaskTray}
      />

      {/* ── Settings ───────────────────────────────────────────────────────── */}
      <NavButton
        icon={<Cog6ToothIcon {...ICON} />}
        label="Settings"
        active={page === "settings"}
        onClick={() => setPage("settings")}
      />
    </div>
  );
}
