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
  ExclamationTriangleIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { ClockIcon } from "@heroicons/react/24/outline";
import { useEffect, useState } from "react";
import { useAppState } from "../../AppContext";
import { LOCAL_CRS } from "../../canvas/coords";
import { engineComponents } from "../../engine/registry";
import {
  attachAuxFile,
  createProjectOnDisk,
  type EngineInfo,
  formatInpImportError,
  type ImportedModel,
  importExtensionLabel,
  isEngineAvailable,
  isEngineGuiOpenable,
  openAndLoadNetwork,
  type Project,
  type SidecarRef,
  useEngines,
  useNetworkVersion,
  type ValidationFinding,
} from "../../hooks";
import { formatIpcError, isTauri } from "../../hooks/ipc";
import { NetworkThumbnail } from "../ui/NetworkThumbnail";
import { PrimaryButton } from "../ui/PrimaryButton";
import { SidecarChecklist } from "../ui/SidecarChecklist";

interface Props {
  onClose: () => void;
  /** A model already read by the recognition path, which chose the engine
   * from the file itself. Absent for the ordinary flow, where the wizard
   * asks for the engine first and filters the picker to its formats. */
  initial?: ImportedModel | null;
}

const TOTAL_STEPS = 3;

/** The review panel's detail column.
 *
 * Left-aligned inside a drop zone that centres everything else, and
 * deliberately so: the heading above it ("Model loaded", the counts) is a
 * result and reads well centred, while everything here is prose and
 * controls, which do not. Ragged-centre radio rows were the whole of what
 * made this panel look broken.
 */
const REVIEW_DETAILS: React.CSSProperties = {
  textAlign: "left",
  marginTop: 16,
  paddingTop: 14,
  borderTop: "1px solid var(--border)",
  display: "flex",
  flexDirection: "column",
  gap: 14,
};

/** Section heading inside the detail column — the same small-caps label the
 * wizard's own step headings use, so the panel reads as part of the page. */
const REVIEW_LABEL: React.CSSProperties = {
  fontSize: "var(--text-xs)",
  fontWeight: 600,
  letterSpacing: "0.06em",
  textTransform: "uppercase",
  color: "var(--text-tertiary)",
  marginBottom: 4,
};

/** For sections the reader must not skim past: what the importer changed,
 * and what still stands between the model and a run. Colour on the label
 * alone — a whole paragraph in amber reads as an alarm, and these are
 * facts. */
const REVIEW_WARN_LABEL: React.CSSProperties = {
  ...REVIEW_LABEL,
  color: "var(--status-warning)",
};

const REVIEW_BODY: React.CSSProperties = {
  fontSize: "var(--text-md)",
  color: "var(--text-secondary)",
  lineHeight: 1.5,
};

/** One answer to the coordinate question, as a row you click rather than a
 * bare radio: the choice carries a sentence of explanation, and a label
 * beside a 13px control wraps into a ragged block that reads as neither. */
function CrsChoice({
  checked,
  onSelect,
  title,
  detail,
}: {
  checked: boolean;
  onSelect: () => void;
  title: string;
  detail: string;
}) {
  return (
    <label
      style={{
        display: "flex",
        gap: 9,
        alignItems: "flex-start",
        cursor: "pointer",
        padding: "8px 10px",
        borderRadius: 7,
        border: `1px solid ${checked ? "var(--accent)" : "var(--border)"}`,
        background: checked ? "var(--selection-bg)" : "transparent",
        transition: "border-color var(--t-fast), background var(--t-fast)",
      }}
    >
      <input
        type="radio"
        name="wizard-crs"
        checked={checked}
        onChange={onSelect}
        style={{ marginTop: 2, flexShrink: 0 }}
      />
      <span>
        <span
          style={{
            display: "block",
            fontSize: "var(--text-md)",
            color: "var(--text-primary)",
            fontWeight: 500,
          }}
        >
          {title}
        </span>
        <span
          style={{
            display: "block",
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            lineHeight: 1.45,
            marginTop: 2,
          }}
        >
          {detail}
        </span>
      </span>
    </label>
  );
}

export function NewProjectWizard({ onClose, initial = null }: Props) {
  const { createProject, showToast } = useAppState();
  const { bumpNetwork } = useNetworkVersion();
  const engines = useEngines();

  // A model chosen before the wizard opened has already answered the first
  // two questions — which engine, and which file — so the wizard opens on
  // the step that reports what was read rather than asking again. It still
  // opens *there* and not on the name: what the importer changed, and the
  // coordinate question, are shown before the project is committed to.
  const [step, setStep] = useState<1 | 2 | 3>(initial ? 2 : 1);
  const [engineKey, setEngineKey] = useState<string | null>(
    initial?.engine ?? null,
  );
  const [projectName, setProjectName] = useState(
    initial?.network.fileStem ?? "",
  );
  const [detecting, setDetecting] = useState(false);
  const [fileDetected, setFileDetected] = useState(initial != null);
  const [detectedNodeCount, setDetectedNodeCount] = useState(
    initial?.nodeCount ?? 0,
  );
  const [detectedLinkCount, setDetectedLinkCount] = useState(
    initial?.linkCount ?? 0,
  );
  // Why the imported model is not yet simulable, empty when it is. Kept so the
  // review step can say so before the user commits: the model imports either
  // way, and finding out only after landing in the project is a worse surprise
  // than being told here.
  const [detectedFindings, setDetectedFindings] = useState<ValidationFinding[]>(
    initial?.findings ?? [],
  );
  // §14.10 repairs the importer applied (nonstandard lines commented out)
  // — shown on the review step; the repair contract forbids silence.
  const [detectedRepairs, setDetectedRepairs] = useState<string[]>(
    initial?.repairs ?? [],
  );
  // Whether the model's coordinates rule out longitude and latitude. The
  // wizard cannot host the real CRS picker — that one proves an answer by
  // showing where the network lands on a basemap, and there is no map here
  // — but it can say so before the map does, and take the one answer that
  // needs no map: a drawing grid is not a coordinate system at all.
  const [coordsProjected, setCoordsProjected] = useState(
    initial?.coordinatesProjected ?? false,
  );
  const [crsAnswer, setCrsAnswer] = useState<"later" | "local">("later");
  // Auxiliary files the model references, each carried or missing — the
  // one thing that decides whether a file-forced drainage model can run
  // at all once it becomes a project.
  const [detectedSidecars, setDetectedSidecars] = useState<SidecarRef[]>(
    initial?.sidecars ?? [],
  );
  const [locating, setLocating] = useState(false);

  const engine = engines.find((e) => e.key === engineKey) ?? null;
  // Engines whose model this GUI cannot edit have no starter-network path —
  // a project can only begin from an imported model.
  const importRequired =
    engine != null && !engineComponents(engine.key).modelEditable;

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
    setDetectedFindings([]);
    setDetectedRepairs([]);
    setCoordsProjected(false);
    setCrsAnswer("later");
    setDetectedSidecars([]);
  }

  /** Point at a referenced data file on disk; the backend refuses one the
   * model never names, so a mis-aimed pick cannot silently do nothing. */
  async function handleLocateAux() {
    if (locating) return;
    setLocating(true);
    try {
      const refreshed = await attachAuxFile();
      if (refreshed) setDetectedSidecars(refreshed);
    } catch (e) {
      showToast(formatIpcError(e), "error");
    } finally {
      setLocating(false);
    }
  }

  async function handleBrowse() {
    if (!engine) return;
    setDetecting(true);
    try {
      const result = await openAndLoadNetwork(engine.key);
      if (result) {
        setDetectedNodeCount(result.nodeCount);
        setDetectedLinkCount(result.linkCount);
        setDetectedFindings(result.findings);
        setDetectedRepairs(result.repairs ?? []);
        setCoordsProjected(result.coordinatesProjected ?? false);
        setCrsAnswer("later");
        setDetectedSidecars(result.sidecars ?? []);
        if (result.repairs?.length) {
          showToast(
            `Imported with ${result.repairs.length} repair${
              result.repairs.length === 1 ? "" : "s"
            } — see the review step for what changed.`,
            "warn",
          );
        }
        setFileDetected(true);
        bumpNetwork();
        if (!projectName.trim() && result.network.fileStem) {
          setProjectName(result.network.fileStem);
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
      // Only adopt the loaded network when this wizard imported one. Managed
      // state may still hold a previously-opened project's model.
      importLoadedNetwork: fileDetected,
    });

    // Inside Tauri, a null answer means the backend refused (the error has
    // already been surfaced through the IPC toast) — fabricating a project
    // card here left the user with a phantom project backed by nothing on
    // disk. The in-memory fallback exists only for the plain-browser dev
    // server.
    if (!persisted && isTauri()) return;

    const project: Project = persisted ?? {
      id,
      name,
      engine: engine.key,
      state: "draft",
      scenarioCount: 0,
      modifiedLabel: "Just now",
      nodeCount: detectedNodeCount,
      linkCount: detectedLinkCount,
      // Mirrors the backend's rule directionally (dev-browser fallback
      // only — a persisted project gets the real answer from
      // source_crs_for_model): projected coordinates are a drawing grid
      // unless the user picks a datum on the map, where the choice can
      // be checked.
      sourceCrs: coordsProjected ? LOCAL_CRS : "EPSG:4326",
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
        // `flex-start` with `margin: auto` on the card, rather than
        // `center`: a centred flex item taller than its container
        // overflows *both* ways, so the head of a long step — the engine
        // list, or a review with repairs and a coordinate question — was
        // cut off above the top of the window with no way to reach it.
        // This way the card centres while it fits and scrolls from its
        // top once it does not.
        alignItems: "flex-start",
        justifyContent: "center",
        overflowY: "auto",
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
          // Centres the card in the leftover space when there is any, and
          // simply sits at the top when there is not.
          margin: "auto",
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
                  // Same gate as the Next button: import-only engines
                  // cannot advance without a source model.
                  if (e.key === "Enter" && !(importRequired && !fileDetected))
                    setStep(3);
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

            <div style={fieldLabelStyle}>
              {importRequired ? "Source model" : "Source model (optional)"}
            </div>
            <p
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
                margin: "-2px 0 8px",
                lineHeight: 1.6,
              }}
            >
              {importRequired
                ? `${engine.label} projects start from an imported model — add one to continue.`
                : "Skip this to start from a single reservoir and build the network in the editor."}
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
                  {/* An unfinished model still loads (model spec §4.1.2), so
                      this is a caveat on a success, not a failure: the heading
                      stays affirmative and the tone shifts to warning. */}
                  {detectedFindings.length > 0 ? (
                    <ExclamationTriangleIcon
                      style={{
                        width: 24,
                        height: 24,
                        marginBottom: 10,
                        color: "var(--status-warning)",
                      }}
                    />
                  ) : (
                    <CheckIcon
                      style={{
                        width: 24,
                        height: 24,
                        marginBottom: 10,
                        color: "var(--status-success)",
                      }}
                    />
                  )}
                  <div
                    style={{
                      fontSize: "var(--text-xl)",
                      color:
                        detectedFindings.length > 0
                          ? "var(--status-warning)"
                          : "var(--status-success)",
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
                  {(coordsProjected ||
                    detectedFindings.length > 0 ||
                    detectedRepairs.length > 0 ||
                    detectedSidecars.length > 0) && (
                    <div style={REVIEW_DETAILS}>
                      <SidecarChecklist
                        sidecars={detectedSidecars}
                        busy={locating}
                        onLocate={() => void handleLocateAux()}
                      />
                      {coordsProjected && (
                        <div>
                          <div style={REVIEW_LABEL}>Coordinates</div>
                          <div style={REVIEW_BODY}>
                            These are not longitude and latitude.
                          </div>
                          <div
                            style={{
                              display: "flex",
                              flexDirection: "column",
                              gap: 6,
                              marginTop: 8,
                            }}
                          >
                            <CrsChoice
                              checked={crsAnswer === "later"}
                              onSelect={() => setCrsAnswer("later")}
                              title="A coordinate system"
                              detail="Choose which one on the map, where you can see where the network lands."
                            />
                            <CrsChoice
                              checked={crsAnswer === "local"}
                              onSelect={() => setCrsAnswer("local")}
                              title="A drawing grid"
                              detail="This model is not placed on the earth."
                            />
                          </div>
                        </div>
                      )}
                      {detectedFindings.length > 0 && (
                        <div>
                          <div style={REVIEW_WARN_LABEL}>Not yet simulable</div>
                          <div style={REVIEW_BODY}>
                            {detectedFindings.length === 1
                              ? "1 issue must be resolved"
                              : `${detectedFindings.length} issues must be resolved`}{" "}
                            before this model can run. The project opens with{" "}
                            {detectedFindings.length === 1 ? "it" : "them"}{" "}
                            listed in Issues &amp; Notifications.
                          </div>
                        </div>
                      )}
                      {detectedRepairs.length > 0 && (
                        <div>
                          <div style={REVIEW_WARN_LABEL}>
                            {detectedRepairs.length === 1
                              ? "1 change made on import"
                              : `${detectedRepairs.length} changes made on import`}
                          </div>
                          <ul
                            style={{
                              ...REVIEW_BODY,
                              margin: "2px 0 0",
                              paddingLeft: 16,
                            }}
                          >
                            {detectedRepairs.map((r) => (
                              <li key={r} style={{ marginTop: 3 }}>
                                {r}
                              </li>
                            ))}
                          </ul>
                        </div>
                      )}
                    </div>
                  )}
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
                <PrimaryButton
                  onClick={() => setStep(3)}
                  disabled={importRequired && !fileDetected}
                  title={
                    importRequired && !fileDetected
                      ? "Import a source model to continue"
                      : undefined
                  }
                  style={
                    importRequired && !fileDetected
                      ? { opacity: 0.5, cursor: "not-allowed" }
                      : undefined
                  }
                >
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
                  {detectedSidecars.filter((s) => s.carried).length > 0 && (
                    <span className="badge">
                      {`+ ${detectedSidecars.filter((s) => s.carried).length} data file${
                        detectedSidecars.filter((s) => s.carried).length === 1
                          ? ""
                          : "s"
                      }`}
                    </span>
                  )}
                </div>
                {detectedSidecars.some((s) => !s.carried) && (
                  <div
                    style={{
                      marginTop: 8,
                      fontSize: "var(--text-sm)",
                      color: "var(--status-warning)",
                    }}
                  >
                    {detectedSidecars.filter((s) => !s.carried).length === 1
                      ? "1 referenced data file is still missing — simulations will refuse until it is supplied."
                      : `${detectedSidecars.filter((s) => !s.carried).length} referenced data files are still missing — simulations will refuse until they are supplied.`}
                  </div>
                )}
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
  const available = isEngineGuiOpenable(engine);
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
          {isEngineAvailable(engine) ? "CLI only" : "Coming soon"}
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
