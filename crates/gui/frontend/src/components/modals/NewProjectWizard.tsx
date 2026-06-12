/**
 * New-project creation wizard rendered as a full-screen modal overlay.
 *
 * Accepts an optional `initialStep` so the "Import INP file…" shortcut can
 * jump straight to the file-picker step. Calls `onClose` when the user
 * cancels and navigates to the new project page when creation succeeds.
 */

import {
  ArrowLeftIcon,
  ArrowRightIcon,
  CheckIcon,
} from "@heroicons/react/16/solid";
import { ClockIcon } from "@heroicons/react/24/outline";
import { useState } from "react";
import { useAppState } from "../../AppContext";
import {
  ACCENT,
  createProjectOnDisk,
  formatInpImportError,
  LABEL,
  openAndLoadNetwork,
  type Project,
  useNetworkVersion,
} from "../../hooks";
import { NetworkThumbnail } from "../ui/NetworkThumbnail";
import { PrimaryButton } from "../ui/PrimaryButton";

interface Props {
  onClose: () => void;
  /** Which wizard step to open on. Defaults to 1. */
  initialStep?: 1 | 2 | 3;
  /** Pre-loaded network (file already opened before the wizard was shown). */
  preloadedNetwork?: { nodes: number; links: number; fileStem: string };
}

export function NewProjectWizard({
  onClose,
  initialStep = 1,
  preloadedNetwork,
}: Props) {
  const { createProject, showToast } = useAppState();
  const { bumpNetwork } = useNetworkVersion();

  const [step, setStep] = useState<1 | 2 | 3>(initialStep);
  const [projectName, setProjectName] = useState(
    preloadedNetwork?.fileStem ?? "",
  );
  const [detecting, setDetecting] = useState(false);
  const [fileDetected, setFileDetected] = useState(!!preloadedNetwork);
  const [detectedNodeCount, setDetectedNodeCount] = useState(
    preloadedNetwork?.nodes ?? 0,
  );
  const [detectedLinkCount, setDetectedLinkCount] = useState(
    preloadedNetwork?.links ?? 0,
  );

  async function handleBrowse() {
    setDetecting(true);
    try {
      const result = await openAndLoadNetwork();
      if (result) {
        setDetectedNodeCount(result.nodes.length);
        setDetectedLinkCount(result.links.length);
        setFileDetected(true);
        bumpNetwork();
        if (!projectName.trim() && result.fileStem) {
          setProjectName(result.fileStem);
        }
      }
    } catch (err) {
      showToast(formatInpImportError(err), "error");
    } finally {
      setDetecting(false);
    }
  }

  async function handleCreate() {
    const id = crypto.randomUUID();
    const name = projectName || "Untitled Project";

    const persisted = await createProjectOnDisk({ id, name });

    const project: Project = persisted ?? {
      id,
      name,
      state: "draft",
      scenarioCount: 0,
      modifiedLabel: "Just now",
      nodeCount: detectedNodeCount,
      linkCount: detectedLinkCount,
      sourceCrs: "EPSG:4326",
      insights: null,
      folderMissing: false,
    };

    createProject(project);
    onClose();
  }

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 600,
        background: "rgba(0,0,0,0.55)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 24,
        animation: "fadeIn 120ms ease-out",
      }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="wizard-card" style={{ width: "100%", maxWidth: 580 }}>
        {/* ── Step 1: Name ───────────────────────────────────────────────── */}
        {step === 1 && (
          <div>
            <h2
              style={{
                margin: "0 0 24px",
                fontSize: 22,
                fontWeight: 700,
                color: "var(--text-primary)",
              }}
            >
              New Project
            </h2>

            <div style={{ marginBottom: 20 }}>
              <label
                htmlFor="new-project-name"
                style={{
                  display: "block",
                  fontSize: 11,
                  fontWeight: 600,
                  color: "var(--text-tertiary)",
                  textTransform: "uppercase",
                  letterSpacing: "0.07em",
                  marginBottom: 8,
                }}
              >
                Project name
              </label>
              <input
                id="new-project-name"
                value={projectName}
                onChange={(e) => setProjectName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") setStep(2);
                  if (e.key === "Escape") onClose();
                }}
                placeholder="e.g. South Side Rehabilitation Study"
                style={{
                  width: "100%",
                  padding: "9px 12px",
                  borderRadius: 7,
                  background: "var(--bg-input)",
                  border: "1px solid var(--border-hover)",
                  color: "var(--text-primary)",
                  fontSize: 14,
                  fontFamily: "var(--font-ui)",
                  outline: "none",
                  boxSizing: "border-box",
                }}
              />
            </div>

            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                marginTop: 32,
              }}
            >
              <button type="button" className="btn-link" onClick={onClose}>
                Cancel
              </button>
              <PrimaryButton onClick={() => setStep(2)}>
                <span
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                  }}
                >
                  Next <ArrowRightIcon style={{ width: 14, height: 14 }} />
                </span>
              </PrimaryButton>
            </div>
          </div>
        )}

        {/* ── Step 2: Import network file ────────────────────────────────── */}
        {step === 2 && (
          <div>
            <h2
              style={{
                margin: "0 0 8px",
                fontSize: 22,
                fontWeight: 700,
                color: "var(--text-primary)",
              }}
            >
              Import Network File
            </h2>
            <p
              style={{
                fontSize: 13,
                color: "var(--text-secondary)",
                margin: "0 0 24px",
                lineHeight: 1.6,
              }}
            >
              Import an existing EPANET INP file, or start with an empty
              network.
            </p>

            <div
              style={{
                display: "flex",
                gap: 10,
                alignItems: "flex-start",
                background: "var(--bg-input)",
                border: "1px solid var(--border)",
                borderRadius: 7,
                padding: "10px 12px",
                marginBottom: 20,
                fontSize: 12,
                color: "var(--text-tertiary)",
                lineHeight: 1.6,
              }}
            >
              <span style={{ flexShrink: 0, fontSize: 14 }}>ℹ</span>
              <span>
                Hydra uses its own hydraulic solver. Results for the same
                network may differ slightly from EPANET — this is expected.
                Hydra defines correctness by its own convergence criteria and
                physical conservation laws, independent of EPANET's output.
              </span>
            </div>

            <div
              style={{
                border: `2px dashed ${fileDetected ? "var(--status-success)" : "var(--border-hover)"}`,
                borderRadius: 10,
                padding: "32px 24px",
                textAlign: "center",
                background: fileDetected
                  ? "rgba(61,175,117,0.07)"
                  : "var(--bg-input)",
                transition:
                  "border-color var(--t-base), background var(--t-base)",
                marginBottom: 20,
              }}
            >
              {detecting ? (
                <div>
                  <ClockIcon
                    style={{
                      width: 24,
                      height: 24,
                      marginBottom: 10,
                      color: "var(--text-tertiary)",
                    }}
                  />
                  <div style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                    Opening file…
                  </div>
                </div>
              ) : fileDetected ? (
                <div>
                  <CheckIcon
                    style={{
                      width: 24,
                      height: 24,
                      marginBottom: 10,
                      color: "var(--status-success)",
                    }}
                  />
                  <div
                    style={{
                      fontSize: 14,
                      color: "var(--status-success)",
                      fontWeight: 600,
                      marginBottom: 4,
                    }}
                  >
                    Network loaded
                  </div>
                  <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                    {detectedNodeCount.toLocaleString()} nodes ·{" "}
                    {detectedLinkCount.toLocaleString()} links
                  </div>
                </div>
              ) : (
                <div>
                  <div
                    style={{
                      fontSize: 28,
                      marginBottom: 10,
                      color: "var(--text-tertiary)",
                    }}
                  >
                    ⊞
                  </div>
                  <div
                    style={{
                      fontSize: 14,
                      color: "var(--text-secondary)",
                      marginBottom: 12,
                    }}
                  >
                    Drop{" "}
                    <code
                      style={{ fontFamily: "var(--font-mono)", fontSize: 13 }}
                    >
                      .inp
                    </code>{" "}
                    file here
                  </div>
                  <button
                    type="button"
                    onClick={handleBrowse}
                    style={{
                      border: "1px solid var(--border-hover)",
                      background: "transparent",
                      color: "var(--text-secondary)",
                      cursor: "pointer",
                      padding: "6px 14px",
                      borderRadius: 6,
                      fontSize: 13,
                      fontFamily: "var(--font-ui)",
                      transition:
                        "background var(--t-fast), color var(--t-fast)",
                    }}
                    onMouseEnter={(e) => {
                      (e.currentTarget as HTMLButtonElement).style.background =
                        "var(--nav-hover)";
                      (e.currentTarget as HTMLButtonElement).style.color =
                        "var(--text-primary)";
                    }}
                    onMouseLeave={(e) => {
                      (e.currentTarget as HTMLButtonElement).style.background =
                        "transparent";
                      (e.currentTarget as HTMLButtonElement).style.color =
                        "var(--text-secondary)";
                    }}
                  >
                    Browse…
                  </button>
                </div>
              )}
            </div>

            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
              }}
            >
              <button
                type="button"
                className="btn-link"
                onClick={() => setStep(1)}
              >
                <span
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                  }}
                >
                  <ArrowLeftIcon style={{ width: 14, height: 14 }} /> Back
                </span>
              </button>
              <div style={{ display: "flex", gap: 10 }}>
                <button
                  type="button"
                  onClick={() => setStep(3)}
                  style={{
                    border: "1px solid var(--border-hover)",
                    background: "transparent",
                    color: "var(--text-secondary)",
                    cursor: "pointer",
                    padding: "8px 16px",
                    borderRadius: 6,
                    fontSize: 13,
                    fontFamily: "var(--font-ui)",
                    transition: "background var(--t-fast), color var(--t-fast)",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLButtonElement).style.background =
                      "var(--nav-hover)";
                    (e.currentTarget as HTMLButtonElement).style.color =
                      "var(--text-primary)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLButtonElement).style.background =
                      "transparent";
                    (e.currentTarget as HTMLButtonElement).style.color =
                      "var(--text-secondary)";
                  }}
                >
                  Skip
                </button>
                <PrimaryButton onClick={() => setStep(3)}>
                  <span
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 4,
                    }}
                  >
                    Next <ArrowRightIcon style={{ width: 14, height: 14 }} />
                  </span>
                </PrimaryButton>
              </div>
            </div>
          </div>
        )}

        {/* ── Step 3: Review + create ────────────────────────────────────── */}
        {step === 3 && (
          <div>
            <h2
              style={{
                margin: "0 0 8px",
                fontSize: 22,
                fontWeight: 700,
                color: "var(--text-primary)",
              }}
            >
              Ready to Create
            </h2>
            <p
              style={{
                fontSize: 13,
                color: "var(--text-secondary)",
                margin: "0 0 24px",
                lineHeight: 1.6,
              }}
            >
              Review your project details before creating.
            </p>

            <div
              style={{
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                borderRadius: 10,
                overflow: "hidden",
                marginBottom: 24,
              }}
            >
              <div
                style={{
                  height: 100,
                  background: "var(--bg-app)",
                  borderBottom: "1px solid var(--border)",
                  overflow: "hidden",
                }}
              >
                <NetworkThumbnail accent={ACCENT} />
              </div>
              <div style={{ padding: "12px 16px" }}>
                <div
                  style={{
                    fontSize: 14,
                    fontWeight: 600,
                    color: "var(--text-primary)",
                    marginBottom: 8,
                  }}
                >
                  {projectName || "Untitled Project"}
                </div>
                <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                  <span
                    className="badge"
                    style={{
                      color: ACCENT,
                      background: `${ACCENT}26`,
                      borderColor: `${ACCENT}55`,
                      fontWeight: 600,
                    }}
                  >
                    {LABEL}
                  </span>
                  {fileDetected && (
                    <span className="badge">
                      {detectedNodeCount.toLocaleString()} nodes
                    </span>
                  )}
                  <span className="badge">Draft</span>
                </div>
              </div>
            </div>

            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
              }}
            >
              <button
                type="button"
                className="btn-link"
                onClick={() => setStep(2)}
              >
                <span
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                  }}
                >
                  <ArrowLeftIcon style={{ width: 14, height: 14 }} /> Back
                </span>
              </button>
              <PrimaryButton onClick={handleCreate}>
                Create Project
              </PrimaryButton>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
