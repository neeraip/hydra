import { openUrl } from "@tauri-apps/plugin-opener";
import { useMemo, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { useAppState } from "../AppContext";
import { NewProjectWizard } from "../components/modals/NewProjectWizard";
import { ReleaseNotesModal } from "../components/modals/ReleaseNotesModal";
import { PrimaryButton } from "../components/ui/PrimaryButton";
import {
  ACCENT,
  engineByKey,
  type Project,
  useEngines,
  useProjects,
} from "../hooks";
import {
  releaseHasNotes,
  releasesWithContent,
  unseenReleases,
  useLastSeenGuiVersion,
  useReleaseNotes,
} from "../hooks/useReleaseNotes";
import { type UpdaterState, useUpdater } from "../hooks/useUpdater";

const HELP_LINKS = [
  {
    label: "Documentation",
    url: "https://neeraip.github.io/hydra/",
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
      ? `Update available — v${updater.version}`
      : updater.phase === "downloading"
        ? `Downloading v${updater.version}…${
            updater.percent !== null ? ` ${updater.percent}%` : ""
          }`
        : updater.phase === "ready"
          ? "Restart to update"
          : updater.phase === "installing"
            ? `Installing v${updater.version}…`
            : updater.phase === "installedNeedsRestart"
              ? `v${updater.version} installed — reopen Hydra`
              : "Update failed — retry";
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
        background: `${ACCENT}14`,
        border: `1px solid ${ACCENT}55`,
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
            `${ACCENT}26`;
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background = `${ACCENT}14`;
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
        <span aria-hidden style={{ fontSize: "var(--text-lg)", lineHeight: 1 }}>
          {updater.phase === "ready" ? "↻" : "↓"}
        </span>
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
            background: `${ACCENT}22`,
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
  const { projectsVersion, createdProject, openProject } = useAppState();
  const notes = useReleaseNotes();
  const { lastSeen, markSeen } = useLastSeenGuiVersion();
  const { updater, install, restart } = useUpdater();
  const [whatsNewOpen, setWhatsNewOpen] = useState(false);

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
      {/* ── Hero ─────────────────────────────────────────────────────────── */}
      <div
        style={{
          flex: "0 0 62%",
          background:
            "linear-gradient(135deg, var(--bg-activity) 0%, var(--bg-elevated) 50%, var(--bg-app) 100%)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          position: "relative",
          overflow: "hidden",
        }}
      >
        {/* Ambient glow */}
        <div
          style={{
            position: "absolute",
            width: 480,
            height: 480,
            borderRadius: "50%",
            background:
              "radial-gradient(circle, rgba(74,144,217,0.14) 0%, transparent 68%)",
            pointerEvents: "none",
          }}
        />

        {/* Content */}
        <div
          style={{
            position: "relative",
            textAlign: "center",
            padding: "0 40px",
          }}
        >
          <div
            style={{
              fontSize: "var(--text-display)",
              fontWeight: 800,
              color: "var(--text-primary)",
              letterSpacing: "-0.04em",
              lineHeight: 1,
              marginBottom: 14,
            }}
          >
            Hydra
          </div>
          <div
            style={{
              fontSize: "var(--text-xl)",
              color: "var(--text-secondary)",
              marginBottom: 40,
              letterSpacing: "0.01em",
              lineHeight: 1.5,
            }}
          >
            A modern platform for water infrastructure simulation.
          </div>
          <div style={{ display: "inline-flex" }}>
            <PrimaryButton onClick={() => setShowWizard(true)}>
              + New project
            </PrimaryButton>
          </div>
        </div>
      </div>

      {/* ── Sidebar ──────────────────────────────────────────────────────── */}
      <div
        style={{
          flex: "0 0 38%",
          background: "var(--bg-panel)",
          borderLeft: "1px solid var(--border)",
          overflow: "auto",
          display: "flex",
          flexDirection: "column",
          padding: "28px 24px",
          gap: 28,
        }}
      >
        {/* Recent projects */}
        <section>
          <SidebarSection title="Recent" />
          {recentProjects.length === 0 ? (
            <div
              style={{
                fontSize: "var(--text-lg)",
                color: "var(--text-tertiary)",
                lineHeight: 1.5,
              }}
            >
              No projects yet. Create one to get started.
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
              {recentProjects.map((p) => {
                const engine = engineByKey(engines, p.engine);
                return (
                  <button
                    type="button"
                    key={p.id}
                    onClick={() => openRecentProject(p)}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 10,
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
                      (e.currentTarget as HTMLButtonElement).style.background =
                        "transparent";
                    }}
                  >
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div
                        style={{
                          fontSize: "var(--text-lg)",
                          fontWeight: 500,
                          color: "var(--text-primary)",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {p.name}
                      </div>
                      <div
                        style={{
                          fontSize: "var(--text-sm)",
                          color: "var(--text-tertiary)",
                          marginTop: 1,
                        }}
                      >
                        {p.modifiedLabel}
                      </div>
                    </div>
                    <span
                      title={engine ? engine.label : "Unsupported engine"}
                      style={{
                        fontSize: "var(--text-xs)",
                        fontWeight: 700,
                        letterSpacing: "0.06em",
                        color: engine?.accent ?? "var(--text-tertiary)",
                        background: `${engine?.accent ?? "#888888"}22`,
                        border: `1px solid ${engine?.accent ?? "#888888"}44`,
                        borderRadius: 4,
                        padding: "2px 6px",
                        flexShrink: 0,
                      }}
                    >
                      {engine?.pill ?? "??"}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </section>

        {/* Divider */}
        <div
          style={{ height: 1, background: "var(--border)", flexShrink: 0 }}
        />

        {/* What's new */}
        <section>
          <SidebarSection title="What's New" />
          <UpdateRow updater={updater} install={install} restart={restart} />
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
                      background: `${ACCENT}22`,
                      border: `1px solid ${ACCENT}44`,
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
              <button
                type="button"
                key={label}
                onClick={() => openUrl(url)}
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
                  (e.currentTarget as HTMLButtonElement).style.background =
                    "transparent";
                }}
              >
                <span
                  style={{
                    fontSize: "var(--text-lg)",
                    color: "var(--text-secondary)",
                  }}
                >
                  {label}
                </span>
                <span
                  style={{
                    fontSize: "var(--text-md)",
                    color: "var(--text-tertiary)",
                  }}
                >
                  ↗
                </span>
              </button>
            ))}
          </div>
        </section>
      </div>

      {showWizard && <NewProjectWizard onClose={() => setShowWizard(false)} />}
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
