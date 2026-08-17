import { ArrowDownTrayIcon, ArrowPathIcon } from "@heroicons/react/24/outline";
import { openUrl } from "@tauri-apps/plugin-opener";
import { lazy, Suspense, useMemo, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { useAppState } from "../AppContext";
import { ImportArchiveWizard } from "../components/modals/ImportArchiveWizard";
import { NewProjectWizard } from "../components/modals/NewProjectWizard";
import { ReleaseNotesModal } from "../components/modals/ReleaseNotesModal";
import { EngineGlyph } from "../components/ui/EngineGlyph";
import { NetworkSketch, type Sketch } from "../components/ui/NetworkSketch";
import { NewProjectButton } from "../components/ui/NewProjectButton";
import { PrimaryButton } from "../components/ui/PrimaryButton";
import { placeholderSketch } from "../components/ui/placeholderSketch";
import {
  ACCENT,
  type ArchiveScan,
  engineByKey,
  formatInpImportError,
  type ImportedModel,
  openAndRecogniseNetwork,
  openAndScanArchive,
  type Project,
  useEngines,
  useProjects,
} from "../hooks";
import { formatIpcError } from "../hooks/ipc";
import { useSketches } from "../hooks/sketches";
import {
  releaseHasNotes,
  releasesWithContent,
  unseenReleases,
  useLastSeenGuiVersion,
  useReleaseNotes,
} from "../hooks/useReleaseNotes";
import { type UpdaterState, useUpdater } from "../hooks/useUpdater";
import { loadLicensesModal } from "../lazyChunks";
import {
  modelSize,
  projectStatus,
  type StatusTone,
} from "./HomePage/projectStatus";

const LicensesModal = lazy(() =>
  loadLicensesModal().then((m) => ({ default: m.LicensesModal })),
);

/** The updater banner's icon, sized to the label it sits beside. */
const UPDATE_ICON: React.CSSProperties = {
  width: "1.15em",
  height: "1.15em",
  flexShrink: 0,
};

// ── Layout ───────────────────────────────────────────────────────────────────

/** The first-run welcome. Full window, because there is nothing else yet. */
const WELCOME: React.CSSProperties = {
  flex: 1,
  background:
    "linear-gradient(135deg, var(--bg-activity) 0%, var(--bg-elevated) 50%, var(--bg-app) 100%)",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  position: "relative",
  overflow: "hidden",
};

const WELCOME_GLOW: React.CSSProperties = {
  position: "absolute",
  width: 480,
  height: 480,
  borderRadius: "50%",
  background:
    "radial-gradient(circle, rgba(205,211,223,0.10) 0%, transparent 68%)",
  pointerEvents: "none",
};

const WORDMARK: React.CSSProperties = {
  fontSize: "var(--text-display)",
  fontWeight: 800,
  color: "var(--text-primary)",
  letterSpacing: "-0.04em",
  lineHeight: 1,
  marginBottom: 14,
};

const TAGLINE: React.CSSProperties = {
  fontSize: "var(--text-xl)",
  color: "var(--text-secondary)",
  marginBottom: 40,
  letterSpacing: "0.01em",
  lineHeight: 1.5,
};

/**
 * The returning shape: the work beside a rail.
 *
 * Two columns rather than one long scroll. Everything the rail holds —
 * an update, what changed, where to get help — was below the fold when
 * this was a single column, which is the same as not being there. The page
 * itself does not scroll; whichever column runs out of room does.
 */
const WORKING: React.CSSProperties = {
  flex: 1,
  minHeight: 0,
  display: "flex",
  background: "var(--bg-app)",
};

const MAIN_COLUMN: React.CSSProperties = {
  flex: 1,
  minWidth: 0,
  overflow: "auto",
  padding: "32px 28px 40px",
  display: "flex",
  flexDirection: "column",
};

/** Narrow and fixed. It holds reading, not work, and a rail that grows with
 *  the window takes room the cards use better. */
const RAIL: React.CSSProperties = {
  flex: "0 0 300px",
  overflow: "auto",
  borderLeft: "1px solid var(--border)",
  background: "var(--bg-panel)",
  padding: "32px 20px 40px",
  display: "flex",
  flexDirection: "column",
  gap: 24,
};

const HEADER_MARK: React.CSSProperties = {
  fontSize: "var(--text-3xl)",
  fontWeight: 800,
  color: "var(--text-primary)",
  letterSpacing: "-0.03em",
  lineHeight: 1.1,
};

const HEADER_SUB: React.CSSProperties = {
  fontSize: "var(--text-md)",
  color: "var(--text-tertiary)",
  marginTop: 2,
};

/** Two across at this measure, one on a narrow window. */
const CARD_GRID: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))",
  gap: 12,
};

const CARD: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  textAlign: "left",
  background: "var(--bg-card)",
  border: "1px solid var(--border)",
  borderRadius: 10,
  padding: 0,
  overflow: "hidden",
  cursor: "pointer",
  font: "inherit",
  transition: "background var(--t-fast), border-color var(--t-fast)",
};

/**
 * The drawing's frame.
 *
 * The ratio sets the height and the contents are taken out of flow, so a
 * card holding a full-size drawing and one holding a small placeholder are
 * the same height. With the contents in flow they were not, and a row of
 * cards stepped up and down depending on what each had to show.
 */
export const CARD_ART: React.CSSProperties = {
  // Stated rather than inherited from being a flex child, so the frame has
  // a width of its own wherever it is put.
  display: "block",
  position: "relative",
  aspectRatio: "16 / 9",
  background: "var(--bg-elevated)",
  borderBottom: "1px solid var(--border)",
  overflow: "hidden",
};

/** Inset from the frame, absolutely, so nothing here can set a height. */
export const CARD_ART_INNER: React.CSSProperties = {
  position: "absolute",
  inset: 14,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
};

const CARD_BODY: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 2,
  padding: "10px 12px 12px",
  minWidth: 0,
};

const CARD_TOP: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 8,
  marginBottom: 4,
};

const ROW_NAME: React.CSSProperties = {
  display: "block",
  fontSize: "var(--text-lg)",
  fontWeight: 500,
  color: "var(--text-primary)",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};

const ROW_SIZE: React.CSSProperties = {
  display: "block",
  fontSize: "var(--text-sm)",
  color: "var(--text-tertiary)",
  marginTop: 1,
};

const ROW_WHEN: React.CSSProperties = {
  fontSize: "var(--text-sm)",
  color: "var(--text-tertiary)",
  flexShrink: 0,
  whiteSpace: "nowrap",
};

const ALL_PROJECTS: React.CSSProperties = {
  alignSelf: "flex-start",
  marginTop: 10,
  background: "transparent",
  border: "none",
  padding: "4px 0",
  cursor: "pointer",
  font: "inherit",
  fontSize: "var(--text-md)",
  color: "var(--text-secondary)",
};

/** Colour follows the engine's own judgement of the state, the way the
 *  legend and the network list already colour a categorical value. */
const TONE_COLOR: Record<StatusTone, string> = {
  quiet: "var(--text-tertiary)",
  attention: "var(--status-warning)",
  alarm: "var(--status-error)",
  busy: "var(--accent)",
};

function statusStyle(tone: StatusTone): React.CSSProperties {
  return {
    fontSize: "var(--text-sm)",
    color: TONE_COLOR[tone],
    flexShrink: 0,
    whiteSpace: "nowrap",
  };
}

const HELP_LINKS = [
  {
    label: "Documentation",
    url: "https://neeraip.github.io/hydra/docs/",
  },
  {
    label: "Community",
    url: "https://github.com/neeraip/hydra/discussions",
  },
  {
    label: "Report a bug",
    url: "https://github.com/neeraip/hydra/issues/new?template=bug_report.yml",
  },
];

/**
 * One row in the Help rail.
 *
 * The trailing mark is what tells the two kinds apart: a link leaves for
 * the browser, and a row without one opens something here. Both are the
 * same row otherwise, so neither reads as the odd one out.
 */
function HelpRow({
  label,
  external,
  onClick,
}: {
  label: string;
  external: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        background: "transparent",
        border: "none",
        cursor: "pointer",
        padding: "8px 10px",
        borderRadius: 6,
        textAlign: "left",
        fontFamily: "var(--font-ui)",
        transition: "background var(--t-fast)",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--nav-hover)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = "transparent";
      }}
    >
      <span
        style={{ fontSize: "var(--text-lg)", color: "var(--text-secondary)" }}
      >
        {label}
      </span>
      {external && (
        <span
          style={{ fontSize: "var(--text-md)", color: "var(--text-tertiary)" }}
        >
          ↗
        </span>
      )}
    </button>
  );
}

// ── What's-new teaser markdown ────────────────────────────────────────────────
// Restricted component map for the clamped sidebar teaser: headings become
// bold lines, lists collapse to compact "·" lines, links render as inert
// text, and images / code blocks / tables are suppressed entirely. The full
// document rendering lives in ReleaseNotesModal.

const TEASER_TEXT: React.CSSProperties = {
  margin: "0 0 3px",
  fontSize: "var(--text-md)",
  lineHeight: 1.55,
};

const TEASER_COMPONENTS: Components = {
  a: ({ children }) => (
    <span style={{ textDecoration: "underline", textUnderlineOffset: 2 }}>
      {children}
    </span>
  ),
  h1: ({ children }) => <div style={TEASER_HEADING}>{children}</div>,
  h2: ({ children }) => <div style={TEASER_HEADING}>{children}</div>,
  h3: ({ children }) => <div style={TEASER_HEADING}>{children}</div>,
  h4: ({ children }) => <div style={TEASER_HEADING}>{children}</div>,
  h5: ({ children }) => <div style={TEASER_HEADING}>{children}</div>,
  h6: ({ children }) => <div style={TEASER_HEADING}>{children}</div>,
  p: ({ children }) => <div style={TEASER_TEXT}>{children}</div>,
  ul: ({ children }) => <div style={TEASER_TEXT}>{children}</div>,
  ol: ({ children }) => <div style={TEASER_TEXT}>{children}</div>,
  li: ({ children }) => <div>· {children}</div>,
  img: () => null,
  pre: () => null,
  table: () => null,
  hr: () => null,
  code: ({ children }) => (
    <code
      style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-sm)" }}
    >
      {children}
    </code>
  ),
};

const TEASER_HEADING: React.CSSProperties = {
  fontWeight: 700,
  color: "var(--text-primary)",
  fontSize: "var(--text-md)",
  margin: "4px 0 2px",
  lineHeight: 1.5,
};

// ── Self-update row ───────────────────────────────────────────────────────────
// Rendered at the top of the What's-new section while an update is
// available / downloading / ready. The teaser + ReleaseNotesModal below it
// remain the what-am-I-getting view; this row only carries the action.

function UpdateRow({
  updater,
  install,
  restart,
}: {
  updater: UpdaterState;
  install: () => void;
  restart: () => void;
}) {
  // Passive phases (nothing to act on) render nothing here — Settings is
  // the surface that reports "checking" / "up to date" / "check failed".
  if (
    updater.phase === "idle" ||
    updater.phase === "checking" ||
    updater.phase === "upToDate" ||
    updater.phase === "checkFailed"
  ) {
    return null;
  }

  // Not actionable while an installer is running, nor once it has finished and
  // only a manual reopen is left — pressing again would start a second one.
  const actionable =
    updater.phase !== "downloading" &&
    updater.phase !== "installing" &&
    updater.phase !== "installedNeedsRestart";
  const label =
    updater.phase === "available"
      ? `Update available: v${updater.version}`
      : updater.phase === "downloading"
        ? `Downloading v${updater.version}…${
            updater.percent !== null ? ` ${updater.percent}%` : ""
          }`
        : updater.phase === "ready"
          ? "Restart to update"
          : updater.phase === "installing"
            ? `Installing v${updater.version}…`
            : updater.phase === "installedNeedsRestart"
              ? `v${updater.version} installed. Reopen Hydra`
              : "Update failed. Retry";
  const sublabel =
    updater.phase === "available"
      ? "Download and install"
      : updater.phase === "ready"
        ? `v${updater.version} is ready to install`
        : updater.phase === "installedNeedsRestart"
          ? "The update is applied and takes effect on next launch"
          : updater.phase === "error"
            ? updater.message
            : null;

  return (
    <button
      type="button"
      disabled={!actionable}
      onClick={updater.phase === "ready" ? restart : install}
      title={updater.phase === "error" ? updater.message : undefined}
      style={{
        display: "block",
        width: "100%",
        background: "var(--accent-dim)",
        border: "1px solid var(--selection-border)",
        borderRadius: 6,
        cursor: actionable ? "pointer" : "default",
        padding: "8px 10px",
        marginBottom: 14,
        textAlign: "left",
        fontFamily: "var(--font-ui)",
        transition: "background var(--t-fast)",
      }}
      onMouseEnter={(e) => {
        if (actionable)
          (e.currentTarget as HTMLButtonElement).style.background =
            "var(--accent-dim)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--accent-dim)";
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          fontSize: "var(--text-md)",
          fontWeight: 600,
          color: ACCENT,
        }}
      >
        {/* Decorative: the label beside it already says what this is, and
            the button carries the action. Sized in `em` so it tracks the
            label through the app's text-size setting. */}
        {updater.phase === "ready" ? (
          <ArrowPathIcon aria-hidden style={UPDATE_ICON} />
        ) : (
          <ArrowDownTrayIcon aria-hidden style={UPDATE_ICON} />
        )}
        <span>{label}</span>
      </div>
      {sublabel && (
        <div
          style={{
            marginTop: 3,
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {sublabel}
        </div>
      )}
      {updater.phase === "downloading" && (
        <div
          style={{
            marginTop: 7,
            height: 3,
            borderRadius: 2,
            background: "var(--selection-bg-strong)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              borderRadius: 2,
              background: ACCENT,
              width: `${updater.percent ?? 100}%`,
              opacity: updater.percent !== null ? 1 : 0.35,
              transition: "width 200ms ease-out",
            }}
          />
        </div>
      )}
    </button>
  );
}

// ── Section header ────────────────────────────────────────────────────────────

function SidebarSection({ title }: { title: string }) {
  return (
    <div
      style={{
        fontSize: "var(--text-xs)",
        fontWeight: 700,
        letterSpacing: "0.1em",
        textTransform: "uppercase",
        color: "var(--text-tertiary)",
        marginBottom: 10,
      }}
    >
      {title}
    </div>
  );
}

// ── Home page ─────────────────────────────────────────────────────────────────

export function HomePage() {
  const {
    projectsVersion,
    createdProject,
    openProject,
    setPage,
    showToast,
    bumpProjects,
  } = useAppState();
  const notes = useReleaseNotes();
  const { lastSeen, markSeen } = useLastSeenGuiVersion();
  const { updater, install, restart } = useUpdater();
  const [whatsNewOpen, setWhatsNewOpen] = useState(false);
  const [licensesOpen, setLicensesOpen] = useState(false);

  const releases = notes.status === "loaded" ? notes.releases : [];
  const latest = releases[0] ?? null;
  const unseen = useMemo(
    () => unseenReleases(releases, lastSeen),
    [releases, lastSeen],
  );
  // Plumbing-only releases (cleaned body empty) stay in the modal accordion
  // as compact rows (hiding them would read as missing versions) but are
  // excluded from the earlier-updates count — that number should promise
  // reading material.
  const unseenWithContent = useMemo(
    () => releasesWithContent(unseen),
    [unseen],
  );
  // Unseen releases with content beyond the newest release.
  const earlierCount = unseenWithContent.filter((r) => r !== latest).length;

  const closeWhatsNew = () => {
    setWhatsNewOpen(false);
    // Everything shown is now seen — advance the marker to the newest
    // fetched version.
    if (latest) markSeen(latest.version);
  };

  const engines = useEngines();
  const backendProjects = useProjects(projectsVersion);
  const recentProjects = useMemo<Project[]>(() => {
    const base =
      createdProject && !backendProjects.some((p) => p.id === createdProject.id)
        ? [createdProject as Project, ...backendProjects]
        : backendProjects;
    return base.slice(0, 5);
  }, [backendProjects, createdProject]);

  const [showWizard, setShowWizard] = useState(false);
  // A scanned archive awaiting review; the modal owns the rest.
  const [archiveScan, setArchiveScan] = useState<ArchiveScan | null>(null);
  // A model recognised before the wizard opened, so it starts from what was
  // read rather than asking for the engine and the file again.
  const [wizardModel, setWizardModel] = useState<ImportedModel | null>(null);

  function startNewProject() {
    setWizardModel(null);
    setShowWizard(true);
  }

  /** Pick a .zip of models and open the review on what the scan found. */
  async function importArchive() {
    try {
      const scan = await openAndScanArchive();
      if (!scan) return; // cancelled
      setArchiveScan(scan);
    } catch (e) {
      showToast(formatIpcError(e), "error");
    }
  }

  /** Open a model file and let it name its own engine (hydra-common
   * §2.5.1), then hand it to the wizard. */
  async function openModelFile() {
    try {
      const model = await openAndRecogniseNetwork();
      if (!model) return; // cancelled
      setWizardModel(model);
      setShowWizard(true);
    } catch (e) {
      showToast(formatInpImportError(e), "error");
    }
  }

  // The page has two shapes. With nothing to return to, the welcome is the
  // whole window; once there is work, the work is.
  const hasProjects = recentProjects.length > 0;
  const sketches = useSketches(recentProjects.map((p) => p.id));

  function openRecentProject(project: Project) {
    // Navigate immediately; AppContext loads and primes network data in the background.
    openProject(project.id);
  }

  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "hidden",
        display: "flex",
        animation: "fadeIn 180ms ease-out",
      }}
    >
      {/* ── Welcome, first run only ─────────────────────────────────────
          Full width while there is nothing else to show. A wordmark and a
          tagline earn the window once; after that the work does. */}
      {!hasProjects && (
        <div style={WELCOME}>
          <div style={WELCOME_GLOW} />
          <div
            style={{
              position: "relative",
              textAlign: "center",
              padding: "0 40px",
            }}
          >
            <div style={WORDMARK}>Hydra</div>
            <div style={TAGLINE}>
              Simulate water distribution and urban drainage networks.
            </div>
            <div style={{ display: "inline-flex", gap: 10 }}>
              <PrimaryButton onClick={startNewProject}>
                + New project
              </PrimaryButton>
              {/* Now does what it says: the file names its own engine, so
                  there is nothing to choose before opening it. */}
              <PrimaryButton
                className="btn-run--outline"
                onClick={() => void openModelFile()}
              >
                Open a model file
              </PrimaryButton>
            </div>
          </div>
        </div>
      )}

      {/* ── The working home ─────────────────────────────────────────────
          One column. What you were doing, then what changed. */}
      {hasProjects && (
        <div style={WORKING}>
          {/* The work. Cards take the room; the rail keeps the
              secondary content in sight without a scroll. */}
          <div style={MAIN_COLUMN}>
            <header
              style={{
                display: "flex",
                alignItems: "baseline",
                justifyContent: "space-between",
                gap: 16,
                marginBottom: 22,
              }}
            >
              <div>
                <div style={HEADER_MARK}>Hydra</div>
                <div style={HEADER_SUB}>
                  Simulate water distribution and urban drainage networks.
                </div>
              </div>
              <div style={{ display: "inline-flex", gap: 8, flexShrink: 0 }}>
                <NewProjectButton
                  size="sm"
                  onNew={startNewProject}
                  onImported={(model) => {
                    setWizardModel(model);
                    setShowWizard(true);
                  }}
                  onArchive={() => void importArchive()}
                  onError={(message) => showToast(message, "error")}
                />
              </div>
            </header>

            <section style={{ marginBottom: 26 }}>
              <SidebarSection title="Recent" />
              {/* A grid rather than a list, because each card carries a
                  drawing of its network and a drawing needs area. Rows with
                  padding would be the projects table with less in it. */}
              <div style={CARD_GRID}>
                {recentProjects.map((p) => {
                  const engine = engineByKey(engines, p.engine);
                  const status = projectStatus(p);
                  const size = modelSize(p);
                  // The real outline when it has been drawn, and a shape
                  // typical of the engine when it has not.
                  const sketch = sketches.get(p.id);
                  const stand = sketch ? null : placeholderSketch(p.engine);
                  return (
                    <button
                      key={p.id}
                      type="button"
                      onClick={() => openRecentProject(p)}
                      style={CARD}
                      onMouseEnter={(e) => {
                        e.currentTarget.style.background =
                          "var(--bg-card-hover)";
                        e.currentTarget.style.borderColor =
                          "var(--border-hover)";
                      }}
                      onMouseLeave={(e) => {
                        e.currentTarget.style.background = "var(--bg-card)";
                        e.currentTarget.style.borderColor = "var(--border)";
                      }}
                    >
                      <span style={CARD_ART}>
                        <span style={CARD_ART_INNER}>
                          {sketch || stand ? (
                            <NetworkSketch
                              sketch={(sketch ?? stand) as Sketch}
                              style={{
                                color: engine?.accent ?? "var(--accent)",
                                // Faint where it stands in for a model we
                                // have not drawn, so nobody reads an invented
                                // network as their own.
                                opacity: sketch ? 1 : 0.28,
                              }}
                            />
                          ) : (
                            <span style={{ opacity: 0.5 }}>
                              <EngineGlyph engine={engine} />
                            </span>
                          )}
                        </span>
                      </span>
                      <span style={CARD_BODY}>
                        <span style={CARD_TOP}>
                          <EngineGlyph engine={engine} size="sm" />
                          <span style={ROW_WHEN}>{p.modifiedLabel}</span>
                        </span>
                        <span style={ROW_NAME}>{p.name}</span>
                        {size && <span style={ROW_SIZE}>{size}</span>}
                        <span style={statusStyle(status.tone)}>
                          {status.label}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
              <button
                type="button"
                onClick={() => setPage("projects")}
                style={ALL_PROJECTS}
              >
                All projects →
              </button>
            </section>
          </div>

          <aside style={RAIL}>
            {/* What's new */}
            <section>
              <SidebarSection title="What's New" />
              <UpdateRow
                updater={updater}
                install={install}
                restart={restart}
              />
              {notes.status === "loading" && (
                <div
                  style={{
                    fontSize: "var(--text-lg)",
                    color: "var(--text-tertiary)",
                    lineHeight: 1.5,
                  }}
                >
                  Loading…
                </div>
              )}
              {notes.status === "unavailable" && (
                <div
                  style={{
                    fontSize: "var(--text-lg)",
                    color: "var(--text-tertiary)",
                    lineHeight: 1.5,
                  }}
                >
                  No release information available.
                </div>
              )}
              {latest && (
                <button
                  type="button"
                  onClick={() => setWhatsNewOpen(true)}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLButtonElement).style.background =
                      "var(--nav-hover)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLButtonElement).style.background =
                      "transparent";
                  }}
                  style={{
                    display: "block",
                    width: "100%",
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    padding: "8px 10px",
                    margin: "-8px -10px",
                    borderRadius: 6,
                    textAlign: "left",
                    fontFamily: "var(--font-ui)",
                    transition: "background var(--t-fast)",
                  }}
                >
                  {/* Header: newest version + date (+ New badge) */}
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      fontSize: "var(--text-md)",
                      fontWeight: 600,
                      color: "var(--text-secondary)",
                    }}
                  >
                    <span>
                      v{latest.version}
                      {latest.date ? ` · ${latest.date}` : ""}
                    </span>
                    {unseen.length > 0 && (
                      <span
                        style={{
                          fontSize: "var(--text-2xs)",
                          fontWeight: 700,
                          letterSpacing: "0.07em",
                          textTransform: "uppercase",
                          color: ACCENT,
                          background: "var(--selection-bg-strong)",
                          border: "1px solid var(--selection-border)",
                          borderRadius: 4,
                          padding: "1px 5px",
                        }}
                      >
                        New
                      </span>
                    )}
                  </div>

                  {/* Clamped markdown teaser with bottom fade — explicit muted
                  empty state when cleanup left no notes. */}
                  {!releaseHasNotes(latest) && (
                    <div
                      style={{
                        marginTop: 7,
                        fontSize: "var(--text-md)",
                        color: "var(--text-tertiary)",
                        lineHeight: 1.55,
                      }}
                    >
                      No release notes
                    </div>
                  )}
                  {releaseHasNotes(latest) && (
                    <div style={{ position: "relative", marginTop: 7 }}>
                      <div
                        style={{
                          display: "-webkit-box",
                          WebkitBoxOrient: "vertical",
                          WebkitLineClamp: 6,
                          overflow: "hidden",
                          // Fallback clamp for engines without -webkit-box:
                          // ~6 lines at 12px/1.55.
                          maxHeight: 6 * 12 * 1.55,
                          color: "var(--text-secondary)",
                        }}
                      >
                        <ReactMarkdown
                          remarkPlugins={[remarkGfm]}
                          components={TEASER_COMPONENTS}
                          skipHtml
                        >
                          {latest.body}
                        </ReactMarkdown>
                      </div>
                      {/* Fade-out into the Read-more affordance */}
                      <div
                        style={{
                          position: "absolute",
                          left: 0,
                          right: 0,
                          bottom: 0,
                          height: 24,
                          background:
                            "linear-gradient(to bottom, transparent, var(--bg-panel))",
                          pointerEvents: "none",
                        }}
                      />
                    </div>
                  )}

                  <div
                    style={{
                      marginTop: 5,
                      fontSize: "var(--text-sm)",
                      color: ACCENT,
                    }}
                  >
                    Read more
                  </div>
                  {earlierCount > 0 && (
                    <div
                      style={{
                        marginTop: 3,
                        fontSize: "var(--text-sm)",
                        color: "var(--text-tertiary)",
                      }}
                    >
                      +{earlierCount} earlier update
                      {earlierCount !== 1 ? "s" : ""}
                    </div>
                  )}
                </button>
              )}
            </section>

            {/* Divider */}
            <div
              style={{ height: 1, background: "var(--border)", flexShrink: 0 }}
            />

            {/* Help links */}
            <section>
              <SidebarSection title="Help" />
              <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
                {HELP_LINKS.map(({ label, url }) => (
                  <HelpRow
                    key={label}
                    label={label}
                    external
                    onClick={() => openUrl(url)}
                  />
                ))}
                {/* Hydra is AGPL software built on nine hundred open-source
                    packages, and a user who never opens Settings should
                    still be one click from knowing that. */}
                <HelpRow
                  label="Licences"
                  external={false}
                  onClick={() => setLicensesOpen(true)}
                />
              </div>
            </section>
          </aside>
        </div>
      )}

      {showWizard && (
        <NewProjectWizard
          initial={wizardModel}
          onClose={() => setShowWizard(false)}
        />
      )}
      {archiveScan && (
        <ImportArchiveWizard
          scan={archiveScan}
          onClose={() => setArchiveScan(null)}
          onDone={(created) => {
            setArchiveScan(null);
            bumpProjects();
            showToast(
              created === 1
                ? "Created 1 project from the archive"
                : `Created ${created} projects from the archive`,
              "success",
            );
          }}
        />
      )}
      {licensesOpen && (
        <Suspense fallback={null}>
          <LicensesModal tab="hydra" onClose={() => setLicensesOpen(false)} />
        </Suspense>
      )}
      {whatsNewOpen && releases.length > 0 && (
        <ReleaseNotesModal
          releases={releases}
          lastSeen={lastSeen}
          onClose={closeWhatsNew}
        />
      )}
    </div>
  );
}
