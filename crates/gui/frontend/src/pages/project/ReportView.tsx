import { DocumentArrowDownIcon, PlusIcon } from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import { RowMenu } from "../../components/ui/RowMenu";
import { ACCENT } from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import {
  type BlockAvailability,
  builderStateFromTemplate,
  buildTemplateJson,
  exportReport,
  generateReport,
  getReportBlockOptions,
  getReportTemplate,
  listReportBlocks,
  moveSection,
  probeReportBlocks,
  type ReportBlockInfo,
  type ReportFormat,
  type ReportOptionInfo,
  readStoredFormat,
  recommendedOrder,
  sameOrder,
  saveReportTemplate,
  writeStoredFormat,
} from "../../hooks/reports";
import { AddSectionPalette } from "./ReportView/AddSectionPalette";
import type { OptionValues } from "./ReportView/BlockOptions";
import { CsvPreview } from "./ReportView/CsvPreview";
import { SectionList } from "./ReportView/SectionList";

const PREVIEW_DEBOUNCE_MS = 350;
const SAVE_DEBOUNCE_MS = 800;

const FORMATS: { id: ReportFormat; label: string }[] = [
  { id: "pdf", label: "PDF" },
  { id: "html", label: "HTML" },
  { id: "csv", label: "CSV" },
  { id: "txt", label: "Text" },
];

/**
 * Report view: template builder (left rail) + per-format document preview.
 *
 * The report target is the app's ACTIVE scenario (the scenario strip in
 * the project toolbar is the one selection mechanism — this view adds no
 * second picker); the document's provenance line names it.
 *
 * The rail is an OUTLINE of the document rather than a list of switches:
 * it holds exactly the sections in the report, in order, because that is
 * what the template records — there is no disabled state in the format, so
 * there is none in the UI. Its state — title, section order, per-section
 * heading and options — IS the template; it round-trips through the same
 * JSON format `hydra report --template` consumes, persisted per project.
 * Each format previews as its own consumer would see it: html rendered in
 * a sandboxed frame, pdf in a viewer, csv as a spreadsheet grid, and text
 * as literal bytes — because a text file's reader really does read it as
 * text. Previews carry no timestamp, so re-renders are stable;
 * the single Export button stamps the generation time and saves via the
 * OS dialog in the selected format.
 */
export function ReportView() {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();

  const [catalog, setCatalog] = useState<ReportBlockInfo[]>([]);
  const [title, setTitle] = useState("Simulation Report");
  // Exactly the sections in the report, in document order — membership is
  // the list, matching the template format, which has no disabled state.
  const [sections, setSections] = useState<string[]>([]);
  // Per-block heading overrides, blank meaning "keep the default".
  const [headingById, setHeadingById] = useState<Record<string, string>>({});
  // Per-block options. Held opaquely so a key this build cannot render — a
  // hand-authored one, or one from a newer engine — survives the round-trip
  // even though the editor only shows described keys.
  const [optionsById, setOptionsById] = useState<Record<string, unknown>>({});
  // Engine-described options per block id, resolved for the active target.
  const [descriptorsById, setDescriptorsById] = useState<
    Record<string, ReportOptionInfo[]>
  >({});
  // Which blocks can actually be produced for this run.
  const [availability, setAvailability] = useState<BlockAvailability[]>([]);
  const [adding, setAdding] = useState(false);
  // Which sections show their settings. Held here rather than in the list so
  // the Sections menu can open or close all of them at once.
  const [openSections, setOpenSections] = useState<Set<string>>(new Set());
  const [format, setFormat] = useState<ReportFormat>("html");
  useEffect(() => {
    if (!activeProjectId) return;
    setFormat(readStoredFormat(activeProjectId, "html"));
  }, [activeProjectId]);

  /** Choose a format and remember it for this project. Written here rather
   * than in an effect on `format`: an effect would also fire when the project
   * changes, and it would still be holding the OUTGOING project's format —
   * storing one project's choice under the next project's key. */
  function chooseFormat(next: ReportFormat) {
    setFormat(next);
    if (activeProjectId) writeStoredFormat(activeProjectId, next);
  }
  const [initialised, setInitialised] = useState(false);

  // Tagged with the format it was generated for: a tab switch must show
  // the loading state, never the previous format's bytes through the
  // wrong presenter (html source in the text pane and vice versa).
  const [preview, setPreview] = useState<{
    format: ReportFormat;
    content: string;
  } | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  // ── Catalog + saved template ───────────────────────────────────────────
  useEffect(() => {
    if (!activeProjectId) return;
    let cancelled = false;
    void (async () => {
      const blocks = await listReportBlocks();
      if (cancelled) return;
      setCatalog(blocks);
      const saved = await getReportTemplate(activeProjectId);
      if (cancelled) return;
      const restored = saved ? builderStateFromTemplate(saved, blocks) : null;
      if (restored) {
        setTitle(restored.title);
        setSections(restored.sections);
        setHeadingById(restored.headingById);
        setOptionsById(restored.optionsById);
      } else {
        // A project with no template starts with every section, which is the
        // most useful default: subtract what you do not want.
        setSections(blocks.map((b) => b.id));
      }
      setInitialised(true);
    })();
    return () => {
      cancelled = true;
    };
  }, [activeProjectId]);

  // Descriptions are model-resolved (defaults and units follow the file's
  // unit system), so they are refetched when the target changes — not
  // cached across scenarios that may declare different units.
  useEffect(() => {
    if (!activeProjectId || catalog.length === 0) return;
    const projectId = activeProjectId;
    const scenarioId = activeScenarioId ?? null;
    let cancelled = false;
    void (async () => {
      const entries = await Promise.all(
        catalog.map(
          async (block) =>
            [
              block.id,
              await getReportBlockOptions(projectId, scenarioId, block.id),
            ] as const,
        ),
      );
      if (cancelled) return;
      setDescriptorsById(Object.fromEntries(entries));
    })();
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, activeScenarioId, catalog]);

  // Which sections can render for this run. One production pass, so it is
  // keyed to the target rather than re-run on every edit.
  useEffect(() => {
    if (!activeProjectId) return;
    const projectId = activeProjectId;
    const scenarioId = activeScenarioId ?? null;
    let cancelled = false;
    void (async () => {
      const probed = await probeReportBlocks(projectId, scenarioId);
      if (!cancelled) setAvailability(probed);
    })();
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, activeScenarioId]);

  const templateJson = useMemo(
    () => buildTemplateJson({ title, sections, headingById, optionsById }),
    [title, sections, headingById, optionsById],
  );

  // ── Live preview (debounced; regenerates on scenario/format change) ────
  useEffect(() => {
    if (!activeProjectId || !initialised) return;
    const projectId = activeProjectId;
    const handle = window.setTimeout(() => {
      generateReport({
        projectId,
        scenarioId: activeScenarioId,
        templateJson,
        format,
        withTimestamp: false,
      })
        .then((rendered) => {
          setPreview({ format, content: rendered });
          setPreviewError(null);
        })
        .catch((err) => {
          setPreviewError(formatIpcError(err));
        });
    }, PREVIEW_DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [activeProjectId, initialised, activeScenarioId, templateJson, format]);

  // ── Template persistence (debounced, best-effort) ──────────────────────
  const skipFirstSave = useRef(true);
  useEffect(() => {
    if (!activeProjectId || !initialised) return;
    if (skipFirstSave.current) {
      skipFirstSave.current = false;
      return;
    }
    const projectId = activeProjectId;
    const handle = window.setTimeout(() => {
      void saveReportTemplate(projectId, templateJson);
    }, SAVE_DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [activeProjectId, initialised, templateJson]);

  // ── Actions ────────────────────────────────────────────────────────────
  function addSection(id: string) {
    setSections((prev) => (prev.includes(id) ? prev : [...prev, id]));
  }

  /** Remove from the document, keeping the section's heading and options in
   * memory so re-adding it during this session restores how it was
   * configured — pulling a section out to see the report without it should
   * not be destructive. Neither is written to the template while the section
   * is absent, so the configuration does not survive a reload. */
  function removeSection(id: string) {
    setSections((prev) => prev.filter((s) => s !== id));
  }

  function reorder(from: number, to: number) {
    setSections((prev) => moveSection(prev, from, to));
  }

  function setHeading(id: string, heading: string) {
    setHeadingById((prev) => ({ ...prev, [id]: heading }));
  }

  function toggleOpen(id: string) {
    setOpenSections((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function setOptions(id: string, next: OptionValues) {
    setOptionsById((prev) => {
      const updated = { ...prev };
      if (next === undefined) delete updated[id];
      else updated[id] = next;
      return updated;
    });
  }

  // ── Bulk actions (the Sections overflow menu) ──────────────────────────
  const catalogIds = useMemo(() => catalog.map((b) => b.id), [catalog]);
  const recommended = useMemo(
    () => recommendedOrder(catalog, sections),
    [catalog, sections],
  );
  const customisedCount = useMemo(
    () =>
      new Set([
        ...Object.keys(optionsById),
        ...Object.keys(headingById).filter((k) => headingById[k]?.trim()),
      ]).size,
    [optionsById, headingById],
  );
  const atDefaults = sameOrder(sections, catalogIds) && customisedCount === 0;
  // "All open" is judged against the CURRENT sections: the set can still hold
  // ids of sections since removed, and those must not decide the label.
  const allExpanded =
    sections.length > 0 && sections.every((id) => openSections.has(id));

  function clearCustomisations() {
    setHeadingById({});
    setOptionsById({});
  }

  function resetEverything() {
    setSections(catalogIds);
    clearCustomisations();
  }

  async function handleExport() {
    if (!activeProjectId || exporting) return;
    setExporting(true);
    try {
      const path = await exportReport({
        projectId: activeProjectId,
        scenarioId: activeScenarioId,
        templateJson,
        format,
      });
      if (path) showToast(`Report saved to ${path}`, "success");
    } catch (err) {
      showToast(formatIpcError(err), "error");
    } finally {
      setExporting(false);
    }
  }

  const blockById = useMemo(
    () => new Map(catalog.map((b) => [b.id, b])),
    [catalog],
  );
  const availabilityById = useMemo(
    () => new Map(availability.map((a) => [a.id, a])),
    [availability],
  );
  const formatLabel =
    FORMATS.find((f) => f.id === format)?.label ?? format.toUpperCase();
  // Only render a preview that matches the selected tab.
  const previewContent =
    preview !== null && preview.format === format ? preview.content : null;
  // Pdf previews arrive as base64 bytes; view them through a blob URL
  // (child-src allows blob:), revoked when replaced.
  const pdfUrl = useMemo(() => {
    if (format !== "pdf" || previewContent === null) return null;
    const bytes = Uint8Array.from(atob(previewContent), (c) => c.charCodeAt(0));
    return URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }));
  }, [format, previewContent]);
  useEffect(() => {
    return () => {
      if (pdfUrl) URL.revokeObjectURL(pdfUrl);
    };
  }, [pdfUrl]);

  return (
    <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
      {/* ── Builder rail ─────────────────────────────────────────────── */}
      <div
        style={{
          flex: "0 0 320px",
          borderRight: "1px solid var(--border)",
          background: "var(--bg-panel)",
          display: "flex",
          flexDirection: "column",
          // The rail itself does not scroll: only the section list inside it
          // does, so the title stays at the top and Export stays reachable at
          // the bottom however long the report gets.
          overflow: "hidden",
          padding: "16px 14px",
          gap: 16,
        }}
      >
        <div style={{ flexShrink: 0 }}>
          <FieldLabel>Report title</FieldLabel>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            spellCheck={false}
            style={{
              width: "100%",
              padding: "6px 8px",
              borderRadius: 6,
              border: "1px solid var(--border-hover)",
              background: "var(--bg-app)",
              color: "var(--text-primary)",
              fontSize: "var(--text-lg)",
              fontFamily: "var(--font-ui)",
            }}
          />
        </div>

        <div
          style={{
            flex: 1,
            minHeight: 0,
            display: "flex",
            flexDirection: "column",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 4,
              flexShrink: 0,
            }}
          >
            <FieldLabel>Sections</FieldLabel>
            <span style={{ display: "flex", alignItems: "center", gap: 2 }}>
              <button
                type="button"
                onClick={() => setAdding((v) => !v)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 3,
                  padding: "2px 6px",
                  borderRadius: 5,
                  border: "1px solid var(--border)",
                  background: "var(--bg-elevated)",
                  color: "var(--text-secondary)",
                  cursor: "pointer",
                  fontSize: "var(--text-sm)",
                  fontFamily: "var(--font-ui)",
                }}
              >
                <PlusIcon style={{ width: 11, height: 11 }} />
                Add
              </button>
              <RowMenu
                label="Section actions"
                // Beside the trigger rather than below it: this menu sits in
                // the Sections header, and dropping downward would cover the
                // outline it acts on.
                placement="right-start"
                items={[
                  {
                    label: "Use recommended order",
                    detail: "Reorders what is in the report; adds nothing",
                    disabled: sameOrder(sections, recommended),
                    disabledReason: "Already in the recommended order",
                    onSelect: () => setSections(recommended),
                  },
                  {
                    label: "Add every section",
                    disabled: sections.length === catalogIds.length,
                    disabledReason: "Every section is already in the report",
                    onSelect: () =>
                      setSections((prev) => [
                        ...prev,
                        ...catalogIds.filter((id) => !prev.includes(id)),
                      ]),
                  },
                  {
                    label: "Clear all settings",
                    detail: "Keeps the sections and their order",
                    disabled: customisedCount === 0,
                    disabledReason: "No section has custom settings",
                    onSelect: clearCustomisations,
                  },
                  {
                    label: allExpanded
                      ? "Collapse all settings"
                      : "Expand all settings",
                    detail: allExpanded
                      ? undefined
                      : "Shows every section's heading and options",
                    disabled: sections.length === 0,
                    disabledReason: "The report has no sections",
                    onSelect: () =>
                      setOpenSections(
                        allExpanded ? new Set() : new Set(sections),
                      ),
                  },
                  {
                    label: "Remove all sections",
                    danger: true,
                    disabled: sections.length === 0,
                    disabledReason: "The report has no sections",
                    onSelect: () => setSections([]),
                  },
                  {
                    label: "Reset report to defaults",
                    detail: "Every section, recommended order, no settings",
                    danger: true,
                    disabled: atDefaults,
                    disabledReason: "The report is already at its defaults",
                    onSelect: resetEverything,
                  },
                ]}
              />
            </span>
          </div>

          {adding ? (
            <div style={{ marginBottom: 8, flexShrink: 0 }}>
              <AddSectionPalette
                catalog={catalog}
                sections={sections}
                availabilityById={availabilityById}
                onAdd={addSection}
                onClose={() => setAdding(false)}
              />
            </div>
          ) : null}

          <div
            style={{
              flex: 1,
              minHeight: 0,
              overflowY: "auto",
              // Reserve the scrollbar's track whether or not one is showing.
              // app.css styles ::-webkit-scrollbar, which opts out of macOS's
              // overlay scrollbars, so the bar takes real width on every
              // platform — and expanding a section far enough to overflow
              // would otherwise narrow every row by 5px, then widen them
              // again on collapse.
              scrollbarGutter: "stable",
            }}
          >
            <SectionList
              sections={sections}
              blockById={blockById}
              descriptorsById={descriptorsById}
              optionsById={optionsById}
              headingById={headingById}
              availabilityById={availabilityById}
              openSections={openSections}
              onToggleOpen={toggleOpen}
              onReorder={reorder}
              onRemove={removeSection}
              onOptionsChange={setOptions}
              onHeadingChange={setHeading}
            />
          </div>
        </div>

        <button
          type="button"
          disabled={exporting}
          onClick={handleExport}
          style={{
            flexShrink: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
            padding: "8px 0",
            borderRadius: 6,
            border: "none",
            background: exporting ? "var(--bg-elevated)" : ACCENT,
            color: exporting ? "var(--text-tertiary)" : "#ffffff",
            cursor: exporting ? "default" : "pointer",
            fontSize: "var(--text-lg)",
            fontWeight: 600,
            fontFamily: "var(--font-ui)",
          }}
        >
          <DocumentArrowDownIcon style={{ width: 14, height: 14 }} />
          {exporting ? "Exporting…" : `Export ${formatLabel}`}
        </button>
      </div>

      {/* ── Preview ──────────────────────────────────────────────────── */}
      <div
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          flexDirection: "column",
          background: "var(--bg-app)",
        }}
      >
        {/* Format tabs: the preview shows the selected format exactly as
            it exports; Export follows the selection. */}
        <div
          style={{
            display: "flex",
            gap: 4,
            padding: "10px 16px",
            borderBottom: "1px solid var(--border)",
            flexShrink: 0,
          }}
        >
          {FORMATS.map((f) => {
            const active = f.id === format;
            return (
              <button
                type="button"
                key={f.id}
                onClick={() => chooseFormat(f.id)}
                style={{
                  padding: "4px 12px",
                  borderRadius: 6,
                  border: "1px solid",
                  borderColor: active ? ACCENT : "transparent",
                  background: active ? `${ACCENT}1a` : "transparent",
                  color: active ? ACCENT : "var(--text-secondary)",
                  cursor: "pointer",
                  fontSize: "var(--text-md)",
                  fontWeight: active ? 600 : 400,
                  fontFamily: "var(--font-ui)",
                  transition: "all var(--t-fast)",
                }}
              >
                {f.label}
              </button>
            );
          })}
        </div>

        <div style={{ flex: 1, minHeight: 0, display: "flex", padding: 20 }}>
          {previewError ? (
            <div
              style={{
                margin: "auto",
                maxWidth: 380,
                textAlign: "center",
                color: "var(--text-tertiary)",
                fontSize: "var(--text-lg)",
                lineHeight: 1.6,
              }}
            >
              {previewError}
            </div>
          ) : previewContent === null ? (
            <div
              style={{
                margin: "auto",
                color: "var(--text-tertiary)",
                fontSize: "var(--text-lg)",
              }}
            >
              Generating preview…
            </div>
          ) : (
            /* The document as it exports: a light "paper" page whatever
               the app theme (the artifact itself is theme-less).

               Only PDF is width-constrained. It is the one format laid out
               for a physical page, so showing it any wider than the paper
               would misrepresent where its lines break. The others reflow to
               whatever they are given — a spreadsheet and a web page have no
               page width, and pinning them to A4 only forced horizontal
               scrolling on tables that would otherwise have fitted. */
            <div
              style={{
                flex: 1,
                minWidth: 0,
                maxWidth: format === "pdf" ? 860 : undefined,
                margin: "0 auto",
                background: "#ffffff",
                borderRadius: 4,
                boxShadow: "0 2px 16px rgba(0,0,0,0.25)",
                overflow: "hidden",
                display: "flex",
              }}
            >
              {format === "pdf" && pdfUrl !== null ? (
                <iframe
                  title="Report preview"
                  src={pdfUrl}
                  style={{ flex: 1, border: "none", background: "#ffffff" }}
                />
              ) : format === "html" ? (
                <iframe
                  title="Report preview"
                  sandbox=""
                  srcDoc={previewContent}
                  style={{ flex: 1, border: "none", background: "#ffffff" }}
                />
              ) : format === "csv" ? (
                <CsvPreview content={previewContent} />
              ) : (
                <pre
                  style={{
                    flex: 1,
                    margin: 0,
                    padding: "20px 24px",
                    overflow: "auto",
                    fontFamily: "var(--font-mono)",
                    fontSize: "var(--text-md)",
                    lineHeight: 1.5,
                    color: "#1a222c",
                    whiteSpace: "pre",
                  }}
                >
                  {previewContent}
                </pre>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        fontSize: "var(--text-xs)",
        fontWeight: 700,
        letterSpacing: "0.08em",
        textTransform: "uppercase",
        color: "var(--text-tertiary)",
        marginBottom: 6,
      }}
    >
      {children}
    </div>
  );
}
