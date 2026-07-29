/**
 * New-project creation wizard rendered as a full-screen modal overlay.
 *
 * This is the *only* way to create a project. An "import a file, infer the
 * rest" shortcut used to exist alongside it, but that path had to assume a
 * modelling domain before the user had named one — so it silently meant
 * "water distribution". With more than one engine registered, the domain is
 * the first thing the user chooses, not something the file extension decides
 * on their behalf (`.inp` belongs to EPANET *and* SWMM).
 *
 * Steps: engine → details (name + optional source model) → review.
 */

import {
  ArrowLeftIcon,
  ArrowRightIcon,
  CheckIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { ClockIcon } from "@heroicons/react/24/outline";
import { useEffect, useState } from "react";
import { useAppState } from "../../AppContext";
import {
  createProjectOnDisk,
  type EngineInfo,
  formatInpImportError,
  importExtensionLabel,
  isEngineAvailable,
  openAndLoadNetwork,
  type Project,
  useEngines,
  useNetworkVersion,
} from "../../hooks";
import { NetworkThumbnail } from "../ui/NetworkThumbnail";
import { PrimaryButton } from "../ui/PrimaryButton";

interface Props {
  onClose: () => void;
}

const TOTAL_STEPS = 3;

export function NewProjectWizard({ onClose }: Props) {
  const { createProject, showToast } = useAppState();
  const { bumpNetwork } = useNetworkVersion();
  const engines = useEngines();

  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [engineKey, setEngineKey] = useState<string | null>(null);
  const [projectName, setProjectName] = useState("");
  const [detecting, setDetecting] = useState(false);
  const [fileDetected, setFileDetected] = useState(false);
  const [detectedNodeCount, setDetectedNodeCount] = useState(0);
  const [detectedLinkCount, setDetectedLinkCount] = useState(0);

  const engine = engines.find((e) => e.key === engineKey) ?? null;

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  function selectEngine(candidate: EngineInfo) {
    if (candidate.key === engineKey) return;
    setEngineKey(candidate.key);
    // A model already imported belongs to the previous engine's format, so
    // it cannot carry over — dropping it here keeps the review step from
    // reporting counts that came from a file this engine never read.
    setFileDetected(false);
    setDetectedNodeCount(0);
    setDetectedLinkCount(0);
  }

  async function handleBrowse() {
    if (!engine) return;
    setDetecting(true);
    try {
      const result = await openAndLoadNetwork(engine.key);
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
    if (!engine) return;
    const id = crypto.randomUUID();
    const name = projectName || "Untitled Project";

    const persisted = await createProjectOnDisk({
      id,
      name,
      engine: engine.key,
    });

    const project: Project = persisted ?? {
      id,
      name,
      engine: engine.key,
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
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the modal on pointer interaction.
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
      <div
        className="wizard-card"
        style={{
          position: "relative",
          width: "100%",
          maxWidth: step === 1 ? 760 : 580,
        }}
      >
        {/* Always-available exit. The footer only offers "Cancel" on step 1 —
            from any later step the leftmost control is "Back", so leaving
            meant stepping backwards until one appeared. */}
        <button
          type="button"
          className="tl-btn"
          onClick={onClose}
          aria-label="Close"
          data-tooltip="Close (Esc)"
          style={{ position: "absolute", top: 14, right: 14 }}
        >
          <XMarkIcon style={{ width: 14, height: 14 }} />
        </button>

        <StepCount step={step} />

        {/* ── Step 1: Modelling domain ───────────────────────────────────── */}
        {step === 1 && (
          <div>
            <h2 style={headingStyle}>What are you modelling?</h2>
            <p style={subheadingStyle}>
              This selects the simulation engine and the model format the
              project imports. It cannot be changed later.
            </p>

            <div
              style={{
                display: "grid",
                gridTemplateColumns: `repeat(${Math.min(engines.length, 3)}, 1fr)`,
                gap: 12,
                marginBottom: 24,
              }}
            >
              {engines.map((candidate) => (
                <EngineCard
                  key={candidate.key}
                  engine={candidate}
                  selected={candidate.key === engineKey}
                  onSelect={() => selectEngine(candidate)}
                />
              ))}
            </div>

            <FooterRow
              left={
                <button type="button" className="btn-link" onClick={onClose}>
                  Cancel
                </button>
              }
              right={
                <PrimaryButton
                  onClick={() => setStep(2)}
                  disabled={engine === null}
                >
                  <NextLabel />
                </PrimaryButton>
              }
            />
          </div>
        )}

        {/* ── Step 2: Name + source model ────────────────────────────────── */}
        {step === 2 && engine && (
          <div>
            <h2 style={headingStyle}>Project Details</h2>
            <p style={subheadingStyle}>
              Name the project and, if you have one, import an existing{" "}
              {engine.import[0]?.label ?? "model file"} to start from.
            </p>

            <div style={{ marginBottom: 20 }}>
              <label htmlFor="new-project-name" style={fieldLabelStyle}>
                Project name
              </label>
              <input
                id="new-project-name"
                value={projectName}
                onChange={(e) => setProjectName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") setStep(3);
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
                  fontSize: "var(--text-xl)",
                  fontFamily: "var(--font-ui)",
                  outline: "none",
                  boxSizing: "border-box",
                }}
              />
            </div>

            <div style={fieldLabelStyle}>Source model (optional)</div>
            <p
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
                margin: "-2px 0 8px",
                lineHeight: 1.6,
              }}
            >
              Skip this to start from a single reservoir and build the network
              in the editor.
            </p>

            <div
              style={{
                border: `2px dashed ${fileDetected ? "var(--status-success)" : "var(--border-hover)"}`,
                borderRadius: 10,
                padding: "28px 24px",
                textAlign: "center",
                background: fileDetected
                  ? "rgba(61,175,117,0.07)"
                  : "var(--bg-input)",
                transition:
                  "border-color var(--t-base), background var(--t-base)",
                marginBottom: 16,
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
                  <div
                    style={{
                      fontSize: "var(--text-lg)",
                      color: "var(--text-secondary)",
                    }}
                  >
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
                      fontSize: "var(--text-xl)",
                      color: "var(--status-success)",
                      fontWeight: 600,
                      marginBottom: 4,
                    }}
                  >
                    Model loaded
                  </div>
                  <div
                    style={{
                      fontSize: "var(--text-md)",
                      color: "var(--text-tertiary)",
                    }}
                  >
                    {detectedNodeCount.toLocaleString()} nodes ·{" "}
                    {detectedLinkCount.toLocaleString()} links
                  </div>
                </div>
              ) : (
                <div>
                  <div
                    style={{
                      fontSize: "var(--text-xl)",
                      color: "var(--text-secondary)",
                      marginBottom: 4,
                    }}
                  >
                    {engine.import[0]?.label ?? "Model file"}
                  </div>
                  <div
                    style={{
                      fontSize: "var(--text-md)",
                      color: "var(--text-tertiary)",
                      fontFamily: "var(--font-mono)",
                      marginBottom: 12,
                    }}
                  >
                    {importExtensionLabel(engine)}
                  </div>
                  <button
                    type="button"
                    onClick={handleBrowse}
                    style={ghostButtonStyle}
                    onMouseEnter={ghostHoverIn}
                    onMouseLeave={ghostHoverOut}
                  >
                    Browse…
                  </button>
                </div>
              )}
            </div>

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
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
                lineHeight: 1.6,
              }}
            >
              <span style={{ flexShrink: 0, fontSize: "var(--text-xl)" }}>
                ℹ
              </span>
              <span>
                Hydra uses its own solver. Results for an imported model may
                differ slightly from the tool it came from — this is expected.
                Hydra defines correctness by its own convergence criteria and
                physical conservation laws.
              </span>
            </div>

            <FooterRow
              left={<BackButton onClick={() => setStep(1)} />}
              right={
                <PrimaryButton onClick={() => setStep(3)}>
                  <NextLabel />
                </PrimaryButton>
              }
            />
          </div>
        )}

        {/* ── Step 3: Review + create ────────────────────────────────────── */}
        {step === 3 && engine && (
          <div>
            <h2 style={headingStyle}>Ready to Create</h2>
            <p style={subheadingStyle}>
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
                <NetworkThumbnail accent={engine.accent} />
              </div>
              <div style={{ padding: "12px 16px" }}>
                <div
                  style={{
                    fontSize: "var(--text-xl)",
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
                      color: engine.accent,
                      background: `${engine.accent}26`,
                      borderColor: `${engine.accent}55`,
                      fontWeight: 600,
                    }}
                  >
                    {engine.label}
                  </span>
                  <span className="badge">
                    {fileDetected
                      ? `${detectedNodeCount.toLocaleString()} nodes`
                      : "Starter network"}
                  </span>
                </div>
              </div>
            </div>

            <FooterRow
              left={<BackButton onClick={() => setStep(2)} />}
              right={
                <PrimaryButton onClick={handleCreate}>
                  Create Project
                </PrimaryButton>
              }
            />
          </div>
        )}
      </div>
    </div>
  );
}

// ── Engine card ──────────────────────────────────────────────────────────────

/**
 * One selectable modelling domain.
 *
 * Planned engines render as real `disabled` buttons carrying a visible
 * "Coming soon" chip rather than being hidden: a user picking a domain
 * deserves to see Hydra's full scope, and a reason that only appears on
 * hover is a reason keyboard users never get.
 */
function EngineCard({
  engine,
  selected,
  onSelect,
}: {
  engine: EngineInfo;
  selected: boolean;
  onSelect: () => void;
}) {
  const available = isEngineAvailable(engine);
  return (
    <button
      type="button"
      onClick={onSelect}
      disabled={!available}
      aria-pressed={selected}
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-start",
        gap: 10,
        textAlign: "left",
        padding: "18px 16px",
        borderRadius: 10,
        background: selected ? `${engine.accent}14` : "var(--bg-input)",
        border: `1.5px solid ${selected ? engine.accent : "var(--border)"}`,
        cursor: available ? "pointer" : "default",
        opacity: available ? 1 : 0.5,
        fontFamily: "var(--font-ui)",
        transition:
          "border-color var(--t-fast), background var(--t-fast), opacity var(--t-fast)",
      }}
      onMouseEnter={(e) => {
        if (available && !selected) {
          e.currentTarget.style.borderColor = "var(--border-hover)";
        }
      }}
      onMouseLeave={(e) => {
        if (available && !selected) {
          e.currentTarget.style.borderColor = "var(--border)";
        }
      }}
    >
      {selected && (
        <CheckIcon
          style={{
            position: "absolute",
            top: 12,
            right: 12,
            width: 16,
            height: 16,
            color: engine.accent,
          }}
        />
      )}

      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          width: 34,
          height: 34,
          borderRadius: 8,
          background: `${engine.accent}26`,
          border: `1px solid ${engine.accent}55`,
          color: engine.accent,
          fontSize: "var(--text-lg)",
          fontWeight: 700,
          letterSpacing: "0.03em",
        }}
      >
        {engine.pill}
      </span>

      <span
        style={{
          fontSize: "var(--text-xl)",
          fontWeight: 600,
          color: "var(--text-primary)",
          lineHeight: 1.3,
        }}
      >
        {engine.label}
      </span>

      <span
        style={{
          fontSize: "var(--text-md)",
          color: "var(--text-secondary)",
          lineHeight: 1.55,
        }}
      >
        {engine.summary}
      </span>

      <span style={{ flex: 1 }} />

      {available ? (
        <span
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {importExtensionLabel(engine)}
        </span>
      ) : (
        <span
          className="badge"
          style={{
            fontSize: "var(--text-xs)",
            fontWeight: 600,
            letterSpacing: "0.04em",
          }}
        >
          Coming soon
        </span>
      )}
    </button>
  );
}

// ── Shared chrome ────────────────────────────────────────────────────────────

function StepCount({ step }: { step: number }) {
  return (
    <div
      style={{
        fontSize: "var(--text-sm)",
        fontWeight: 600,
        color: "var(--text-tertiary)",
        textTransform: "uppercase",
        letterSpacing: "0.07em",
        marginBottom: 10,
      }}
    >
      Step {step} of {TOTAL_STEPS}
    </div>
  );
}

function FooterRow({
  left,
  right,
}: {
  left: React.ReactNode;
  right: React.ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        marginTop: 8,
      }}
    >
      {left}
      {right}
    </div>
  );
}

function BackButton({ onClick }: { onClick: () => void }) {
  return (
    <button type="button" className="btn-link" onClick={onClick}>
      <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
        <ArrowLeftIcon style={{ width: 14, height: 14 }} /> Back
      </span>
    </button>
  );
}

function NextLabel() {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
      Next <ArrowRightIcon style={{ width: 14, height: 14 }} />
    </span>
  );
}

const headingStyle: React.CSSProperties = {
  margin: "0 0 8px",
  fontSize: "var(--text-3xl)",
  fontWeight: 700,
  color: "var(--text-primary)",
};

const subheadingStyle: React.CSSProperties = {
  fontSize: "var(--text-lg)",
  color: "var(--text-secondary)",
  margin: "0 0 24px",
  lineHeight: 1.6,
};

const fieldLabelStyle: React.CSSProperties = {
  display: "block",
  fontSize: "var(--text-sm)",
  fontWeight: 600,
  color: "var(--text-tertiary)",
  textTransform: "uppercase",
  letterSpacing: "0.07em",
  marginBottom: 8,
};

const ghostButtonStyle: React.CSSProperties = {
  border: "1px solid var(--border-hover)",
  background: "transparent",
  color: "var(--text-secondary)",
  cursor: "pointer",
  padding: "6px 14px",
  borderRadius: 6,
  fontSize: "var(--text-lg)",
  fontFamily: "var(--font-ui)",
  transition: "background var(--t-fast), color var(--t-fast)",
};

function ghostHoverIn(e: React.MouseEvent<HTMLButtonElement>) {
  e.currentTarget.style.background = "var(--nav-hover)";
  e.currentTarget.style.color = "var(--text-primary)";
}

function ghostHoverOut(e: React.MouseEvent<HTMLButtonElement>) {
  e.currentTarget.style.background = "transparent";
  e.currentTarget.style.color = "var(--text-secondary)";
}
