import {
  BoltIcon,
  Cog6ToothIcon,
  FolderIcon,
} from "@heroicons/react/24/outline";
import { useActiveProject, useAppState, useTasks } from "../../AppContext";
import logoGlyphUrl from "../../assets/logo-glyph.png";
import { PROJECT_VIEWS } from "../../hooks";
import { loadSettingsDrawer } from "../../lazyChunks";
import { formatPrimaryShortcut, isMacLikePlatform } from "../../shortcuts";
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
    toggleSettings,
    settingsOpen,
    activeProjectId,
  } = useAppState();
  const { project } = useActiveProject();
  const tasks = useTasks();

  const runningCount = tasks.filter((t) => t.status === "running").length;
  const failedCount = tasks.filter((t) => t.status === "failed").length;
  const hasActivity = runningCount > 0 || failedCount > 0;
  const commandPaletteShortcut = formatPrimaryShortcut("K");
  const isMac = isMacLikePlatform();

  // Home goes home. It used to divert to Projects whenever a project was
  // open — "up one level" behaviour under a control that says Home, in a rail
  // where Projects is already its own item one row below. A control that does
  // different things depending on where you stand is the thing to avoid, and
  // for anyone reading the label rather than the picture it was simply wrong.
  //
  // The project stays open, so the rail keeps its sub-nav and going back is
  // one click; leaving is what the project's own close does.
  function handleHomeClick() {
    setPage("home");
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
          // No plate: the rail's other items are bare icons, and one in the
          // accent would make the app's own logo read as a selected control.
          background: "transparent",
          color: "var(--text-primary)",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
          padding: 0,
        }}
      >
        {/*
          The mark itself, lifted out of the app icon.

          `icons/logo.png` is a plated icon — a black rounded tile with the
          glyph knocked out of it — which is right for a dock and wrong for a
          nav rail, where every neighbour is a bare monochrome icon and no
          plate would follow the theme. So the glyph is extracted to an alpha
          mask (ImageMagick: flatten on black, intensity to alpha, threshold
          off the tile's antialiased edge, trim) and painted with the current
          text colour. One asset, both themes, no inverted copy to keep in
          step — and hover and focus can tint it like anything else.
        */}
        <span
          aria-hidden="true"
          style={{
            width: 21,
            height: 21,
            display: "block",
            backgroundColor: "currentColor",
            WebkitMaskImage: `url(${logoGlyphUrl})`,
            maskImage: `url(${logoGlyphUrl})`,
            WebkitMaskSize: "contain",
            maskSize: "contain",
            WebkitMaskRepeat: "no-repeat",
            maskRepeat: "no-repeat",
            WebkitMaskPosition: "center",
            maskPosition: "center",
          }}
        />
      </button>

      {/* ── Global nav ─────────────────────────────────────────────────────── */}
      <div className="divider" />
      <NavButton
        icon={<FolderIcon {...ICON} />}
        label="Projects"
        active={page === "projects"}
        onClick={() => setPage("projects")}
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
        data-tooltip={`Command Palette (${commandPaletteShortcut}) · Shortcuts (?)`}
        data-tooltip-pos="right"
        aria-label="Command Palette"
        className="cmd-palette-btn"
        style={{
          width: isMac ? 32 : 50,
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
          fontSize: "var(--text-sm)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {commandPaletteShortcut}
      </button>

      {/* ── Task monitor ───────────────────────────────────────────────────── */}
      <NavButton
        icon={<BoltIcon {...ICON} />}
        label={`Task Monitor${hasActivity ? " (tasks in progress)" : ""}`}
        active={taskTrayOpen}
        badgeCount={failedCount > 0 ? failedCount : undefined}
        pulse={runningCount > 0 && failedCount === 0}
        onClick={toggleTaskTray}
      />

      {/* ── Settings ───────────────────────────────────────────────────────── */}
      <NavButton
        icon={<Cog6ToothIcon {...ICON} />}
        label="Settings"
        active={settingsOpen}
        onClick={toggleSettings}
        // Belt and braces with the idle prefetch in `App`: a pointer
        // arriving here is the earliest reliable signal that the drawer is
        // about to be wanted, and it lands a few hundred milliseconds
        // before the click. Repeat calls are free.
        onPrefetch={() => void loadSettingsDrawer()}
      />
    </div>
  );
}
