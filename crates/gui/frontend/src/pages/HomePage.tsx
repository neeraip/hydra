import { useMemo, useState } from "react";
import { useAppState } from "../AppContext";
import { NewProjectWizard } from "../components/modals/NewProjectWizard";
import { SplitActionButton } from "../components/ui/SplitActionButton";
import {
  ACCENT,
  createProjectOnDisk,
  formatInpImportError,
  loadProject,
  openAndLoadNetwork,
  PILL,
  type Project,
  useNetworkVersion,
  useProjects,
} from "../hooks";
import { useLatestRelease } from "../hooks/useLatestRelease";

const HELP_LINKS = [
  {
    label: "Documentation",
    url: "https://github.com/neeraip/hydra/wiki",
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

// ── Section header ────────────────────────────────────────────────────────────

function SidebarSection({ title }: { title: string }) {
  return (
    <div
      style={{
        fontSize: 10,
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
    enterLoadedProject,
    createProject,
    showToast,
  } = useAppState();
  const { bumpNetwork } = useNetworkVersion();
  const release = useLatestRelease();

  const backendProjects = useProjects(projectsVersion);
  const recentProjects = useMemo<Project[]>(() => {
    const base =
      createdProject && !backendProjects.some((p) => p.id === createdProject.id)
        ? [createdProject as Project, ...backendProjects]
        : backendProjects;
    return base.slice(0, 5);
  }, [backendProjects, createdProject]);

  const [showWizard, setShowWizard] = useState(false);

  async function handleImportInp() {
    try {
      const result = await openAndLoadNetwork();
      if (!result) return;
      bumpNetwork();
      const id = crypto.randomUUID();
      const name = result.fileStem || "Imported Project";
      const persisted = await createProjectOnDisk({ id, name });
      const project: Project = persisted ?? {
        id,
        name,
        state: "ready",
        scenarioCount: 0,
        modifiedLabel: "Just now",
        nodeCount: result.nodes.length,
        linkCount: result.links.length,
        sourceCrs: "EPSG:4326",
        insights: null,
        folderMissing: false,
      };
      createProject(project);
    } catch (err) {
      showToast(formatInpImportError(err), "error");
    }
  }

  async function openRecentProject(project: Project) {
    const loaded = await loadProject(project.id);
    if (loaded) {
      if (loaded.network) bumpNetwork();
      enterLoadedProject(loaded.project);
    } else {
      openProject(project.id);
    }
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
              fontSize: 72,
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
              fontSize: 15,
              color: "var(--text-secondary)",
              marginBottom: 40,
              letterSpacing: "0.01em",
              lineHeight: 1.5,
            }}
          >
            A modern engine for water distribution simulation.
          </div>
          <div style={{ display: "inline-flex" }}>
            <SplitActionButton
              label="+ New project"
              onClick={() => setShowWizard(true)}
              menuItems={[
                { label: "Import INP file…", onClick: handleImportInp },
              ]}
            />
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
                fontSize: 13,
                color: "var(--text-tertiary)",
                lineHeight: 1.5,
              }}
            >
              No projects yet. Create one to get started.
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
              {recentProjects.map((p) => {
                return (
                  <button
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
                          fontSize: 13,
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
                          fontSize: 11,
                          color: "var(--text-tertiary)",
                          marginTop: 1,
                        }}
                      >
                        {p.modifiedLabel}
                      </div>
                    </div>
                    <span
                      style={{
                        fontSize: 10,
                        fontWeight: 700,
                        letterSpacing: "0.06em",
                        color: ACCENT,
                        background: `${ACCENT}22`,
                        border: `1px solid ${ACCENT}44`,
                        borderRadius: 4,
                        padding: "2px 6px",
                        flexShrink: 0,
                      }}
                    >
                      {PILL}
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
          {release.status === "loading" && (
            <div
              style={{ fontSize: 13, color: "var(--text-tertiary)", lineHeight: 1.5 }}
            >
              Loading…
            </div>
          )}
          {release.status === "unavailable" && (
            <div
              style={{ fontSize: 13, color: "var(--text-tertiary)", lineHeight: 1.5 }}
            >
              No release information available.
            </div>
          )}
          {release.status === "loaded" && (
            <>
              <div
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  color: "var(--text-secondary)",
                  marginBottom: 10,
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                }}
              >
                <span>
                  v{release.version}
                  {release.date ? ` · ${release.date}` : ""}
                </span>
                <button
                  onClick={() =>
                    window.open(release.releaseUrl, "_blank", "noopener")
                  }
                  style={{
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    fontSize: 11,
                    color: ACCENT,
                    padding: 0,
                    fontFamily: "var(--font-ui)",
                  }}
                >
                  ↗ Release notes
                </button>
              </div>
              {release.items.length > 0 ? (
                <ul
                  style={{
                    margin: 0,
                    padding: "0 0 0 16px",
                    display: "flex",
                    flexDirection: "column",
                    gap: 5,
                  }}
                >
                  {release.items.map((item, i) => (
                    <li
                      key={i}
                      style={{
                        fontSize: 13,
                        color: "var(--text-secondary)",
                        lineHeight: 1.5,
                      }}
                    >
                      {item}
                    </li>
                  ))}
                </ul>
              ) : (
                <div
                  style={{ fontSize: 13, color: "var(--text-tertiary)", lineHeight: 1.5 }}
                >
                  See release notes for details.
                </div>
              )}
            </>
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
                key={label}
                onClick={() => window.open(url, "_blank", "noopener")}
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
                <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                  {label}
                </span>
                <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                  ↗
                </span>
              </button>
            ))}
          </div>
        </section>
      </div>

      {showWizard && <NewProjectWizard onClose={() => setShowWizard(false)} />}
    </div>
  );
}

// ── Split action button ───────────────────────────────────────────────────────
