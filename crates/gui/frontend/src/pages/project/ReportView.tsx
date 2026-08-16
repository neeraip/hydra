import {
  BarsArrowDownIcon,
  ChevronRightIcon,
  DocumentArrowDownIcon,
  PlusIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import { RowMenu } from "../../components/ui/RowMenu";
import { ACCENT } from "../../hooks";
import { fetchInto } from "../../hooks/fetchInto";
import { formatIpcError } from "../../hooks/ipc";
import {
  type BlockAvailability,
  builderStateFromTemplate,
  buildTemplateJson,
  exportReport,
  generateReport,
  getReportBlockOptions,
  getReportTemplate,
  lineStartOffset,
  listReportBlocks,
  moveSection,
  probeReportBlocks,
  producibleSections,
  type ReportBlockInfo,
  type ReportFormat,
  type ReportOptionInfo,
  readStoredFormat,
  recommendedOrder,
  sameOrder,
  saveReportTemplate,
  txtHeadingLine,
  unproducibleSections,
  withRecommendedPlacement,
  writeStoredFormat,
} from "../../hooks/reports";
import { useSimulation } from "../../SimulationContext";
import { useUnitSystem } from "../../units";
import { AddSectionPalette } from "./ReportView/AddSectionPalette";
import type { OptionValues } from "./ReportView/BlockOptions";
import { CsvPreview, type CsvPreviewHandle } from "./ReportView/CsvPreview";
import { SectionList } from "./ReportView/SectionList";

/** The header row's icon buttons. Shared so the pair cannot drift apart —
 * they sit side by side, where a pixel of disagreement is obvious. */
const headerIconButton: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 24,
  height: 24,
  padding: 0,
  borderRadius: 5,
  border: "1px solid var(--border)",
  background: "var(--bg-elevated)",
  color: "var(--text-secondary)",
};

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
  // Freshness token: bumps when a run lands. Without it the page keeps
  // whatever it worked out before the run — a project simulated for the first
  // time goes on reporting that it has no results until it is reloaded.
  const { resultGeneration } = useSimulation();

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
  // the header's disclosure button can open or close all of them at once.
  const [openSections, setOpenSections] = useState<Set<string>>(new Set());
  const [format, setFormat] = useState<ReportFormat>("html");
  useEffect(() => {
    if (!activeProjectId) return;
    setFormat(readStoredFormat(activeProjectId, "html"));
    // A different project is a different document: carrying the old offset
    // over would land the reader at the wrong end of a shorter report. Only
    // scenario switches, which this preservation exists for, keep it.
    htmlScrollY.current = 0;
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

  // Scroll position of the HTML preview, kept across the document swaps that
  // a scenario change causes. Each swap is a fresh document with a fresh
  // window, so the listener is reattached on every load.
  const htmlFrameRef = useRef<HTMLIFrameElement>(null);
  const htmlScrollY = useRef(0);
  const detachHtmlScroll = useRef<(() => void) | null>(null);

  function restoreHtmlScroll() {
    const win = htmlFrameRef.current?.contentWindow;
    if (!win) return;
    detachHtmlScroll.current?.();
    win.scrollTo(0, htmlScrollY.current);
    const onScroll = () => {
      htmlScrollY.current = win.scrollY;
    };
    win.addEventListener("scroll", onScroll, { passive: true });
    detachHtmlScroll.current = () =>
      win.removeEventListener("scroll", onScroll);
  }

  useEffect(() => () => detachHtmlScroll.current?.(), []);
  const [exporting, setExporting] = useState(false);

  const csvRef = useRef<CsvPreviewHandle>(null);
  const txtRef = useRef<HTMLPreElement>(null);

  // ── Catalog + saved template ───────────────────────────────────────────
  useEffect(() => {
    if (!activeProjectId) return;
    let cancelled = false;
    void (async () => {
      const blocks = await listReportBlocks(activeProjectId);
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: `resultGeneration` is an intentional retrigger — option defaults are resolved against the run.
  useEffect(() => {
    if (!activeProjectId || catalog.length === 0) return;
    const projectId = activeProjectId;
    const scenarioId = activeScenarioId ?? null;
    return fetchInto(
      Promise.all(
        catalog.map(
          async (block) =>
            [
              block.id,
              await getReportBlockOptions(projectId, scenarioId, block.id),
            ] as const,
        ),
      ),
      (entries) => setDescriptorsById(Object.fromEntries(entries)),
    );
  }, [activeProjectId, activeScenarioId, catalog, resultGeneration]);

  // Which sections can render for this run. One production pass, so it is
  // keyed to the target rather than re-run on every edit.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `resultGeneration` is an intentional retrigger — which sections can render depends on the run, so it must be reprobed when one completes.
  useEffect(() => {
    if (!activeProjectId) return;
    const projectId = activeProjectId;
    const scenarioId = activeScenarioId ?? null;
    return fetchInto(probeReportBlocks(projectId, scenarioId), setAvailability);
  }, [activeProjectId, activeScenarioId, resultGeneration]);

  const templateJson = useMemo(
    () =>
      buildTemplateJson({ title, sections, headingById, optionsById }, catalog),
    [title, sections, headingById, optionsById, catalog],
  );

  // The reader's resolved display system: the preview and the exported
  // document follow it, so what you read is what you save.
  const unitSystem = useUnitSystem();

  // ── Live preview (debounced; regenerates on scenario/format change) ────
  // biome-ignore lint/correctness/useExhaustiveDependencies: `resultGeneration` is an intentional retrigger — the preview renders from the run's results, so a completed run must regenerate it.
  useEffect(() => {
    if (!activeProjectId || !initialised) return;
    const projectId = activeProjectId;
    // Clearing the timeout only cancels a generation that has not started.
    // One already in flight still resolves, and a report of any size takes
    // long enough for the user to have moved on — so a superseded run's
    // answer is dropped rather than written. The `preview.format` check at
    // the render site cannot do this job: it asks whether the *format*
    // matches, and the preview also depends on the scenario, the template,
    // the units and the run, so a stale answer that happens to share the
    // format passed it and put another scenario's report on screen.
    let cancelled = false;
    const handle = window.setTimeout(() => {
      generateReport({
        projectId,
        scenarioId: activeScenarioId,
        templateJson,
        format,
        withTimestamp: false,
        unitSystem,
      })
        .then((rendered) => {
          if (cancelled) return;
          setPreview({ format, content: rendered });
          setPreviewError(null);
        })
        .catch((err) => {
          if (cancelled) return;
          setPreviewError(formatIpcError(err));
        });
    }, PREVIEW_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [
    activeProjectId,
    initialised,
    activeScenarioId,
    templateJson,
    format,
    resultGeneration,
    unitSystem,
  ]);

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
  // Whether the outline already reads in the recommended order, which is the
  // only thing the sort button would change.
  const orderIsRecommended = sameOrder(sections, recommended);
  const customisedCount = useMemo(
    () =>
      new Set([
        ...Object.keys(optionsById),
        ...Object.keys(headingById).filter((k) => headingById[k]?.trim()),
      ]).size,
    [optionsById, headingById],
  );
  // The state "Reset report to every section" restores: the whole catalog in
  // catalog order, with nothing customised.
  const atEverySection =
    sameOrder(sections, catalogIds) && customisedCount === 0;
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
        unitSystem,
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
  // Catalog sections the engine says DO produce something for this target.
  // Empty before a run: nothing probed means nothing known to be available.
  const producible = useMemo(
    () => producibleSections(catalogIds, availabilityById),
    [catalogIds, availabilityById],
  );
  // Sections the engine says cannot render for this target.
  const barren = useMemo(
    () => unproducibleSections(sections, availabilityById),
    [sections, availabilityById],
  );
  const formatLabel =
    FORMATS.find((f) => f.id === format)?.label ?? format.toUpperCase();
  // Only render a preview that matches the selected tab.
  const previewContent =
    preview !== null && preview.format === format ? preview.content : null;
  /** Why the preview cannot be scrolled to a section, or null when it can. */
  const revealBlocked =
    preview?.format !== format || previewContent === null
      ? "The preview is still rendering"
      : format === "pdf"
        ? "The PDF viewer cannot be scrolled to a section"
        : null;

  /** Scroll the preview to the section at `index` in the outline.
   *
   * Located by position rather than by any anchor in the output: every
   * section emits exactly one `<h2>` in html and one `#` row in csv,
   * whatever its variant, so the Nth of those IS the Nth section. That keeps
   * the renderers — a compatibility surface with golden tests — untouched.
   * Only txt lacks a positional handle, because it rules table headers with
   * the same dashes it underlines titles with, so it matches on the heading
   * text instead. */
  function revealSection(index: number) {
    if (revealBlocked) return;
    if (format === "html") {
      const heading =
        htmlFrameRef.current?.contentDocument?.querySelectorAll("h2")[index];
      heading?.scrollIntoView({ behavior: "smooth", block: "start" });
      return;
    }
    if (format === "csv") {
      csvRef.current?.scrollToSection(index);
      return;
    }
    const pre = txtRef.current;
    const id = sections[index];
    if (!pre || !id || previewContent === null) return;
    const title = headingById[id]?.trim() || blockById.get(id)?.title || id;
    const line = txtHeadingLine(previewContent, title);
    if (line === null) return;

    // Ask the browser where the line actually is, rather than multiplying the
    // line number by a line height. That multiplication compounds every
    // discrepancy between the computed line height and the laid-out one — and
    // it ignores the block's top padding — so the first sections land close
    // enough to look correct while later ones drift steadily past their mark.
    // A Range over the line's first character reports its true position, which
    // is exact however the text is laid out.
    const node = pre.firstChild;
    if (!(node instanceof Text)) return;
    const offset = lineStartOffset(previewContent, line);
    if (offset >= node.length) return;
    const range = document.createRange();
    range.setStart(node, offset);
    range.setEnd(node, offset + 1);
    const top =
      range.getBoundingClientRect().top -
      pre.getBoundingClientRect().top +
      pre.scrollTop;
    pre.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
  }

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
              {/* Out of the overflow menu too: restoring the reading order is
                  a one-shot arrangement of what is already there, and it was
                  the only item in that menu you might reach for repeatedly
                  while shuffling sections about. Icon-only, because the row
                  already carries one labelled button. */}
              <button
                type="button"
                aria-label="Use recommended order"
                data-tooltip={
                  orderIsRecommended
                    ? "Already in the recommended order"
                    : "Use recommended order"
                }
                disabled={orderIsRecommended}
                onClick={() => setSections(recommended)}
                style={{
                  ...headerIconButton,
                  cursor: orderIsRecommended ? "default" : "pointer",
                  opacity: orderIsRecommended ? 0.5 : 1,
                }}
              >
                <BarsArrowDownIcon style={{ width: 12, height: 12 }} />
              </button>
              {/* Out of the overflow menu and into the row: expanding the
                  outline is a view toggle used while reading it, not a
                  one-shot edit like the actions it sat among, and a menu is a
                  poor home for something you flip back and forth.
                  Icon-only, because the row already carries a labelled
                  button and a second one would crowd it — and the rotating
                  chevron is the same disclosure vocabulary each section row
                  uses, so it reads as "all of those at once". */}
              <button
                type="button"
                aria-label={
                  allExpanded ? "Collapse all settings" : "Expand all settings"
                }
                data-tooltip={
                  allExpanded ? "Collapse all settings" : "Expand all settings"
                }
                disabled={sections.length === 0}
                onClick={() =>
                  setOpenSections(allExpanded ? new Set() : new Set(sections))
                }
                style={{
                  ...headerIconButton,
                  cursor: sections.length === 0 ? "default" : "pointer",
                  opacity: sections.length === 0 ? 0.5 : 1,
                }}
              >
                <ChevronRightIcon
                  style={{
                    width: 12,
                    height: 12,
                    transform: allExpanded ? "rotate(90deg)" : undefined,
                    transition: "transform var(--t-fast)",
                  }}
                />
              </button>
              <RowMenu
                label="Section actions"
                // Solid, to match the buttons it sits beside: a borderless
                // trigger next to bordered ones reads as secondary.
                variant="solid"
                // Beside the trigger rather than below it: this menu sits in
                // the Sections header, and dropping downward would cover the
                // outline it acts on.
                placement="right-start"
                items={[
                  {
                    label: "Add every section",
                    detail:
                      "New sections go where they read; existing ones stay put",
                    disabled: sections.length === catalogIds.length,
                    disabledReason: "Every section is already in the report",
                    // Placed by recommendation rather than appended: adding
                    // everything to a report holding only Pipe Criticality
                    // would otherwise put Run Summary underneath it.
                    onSelect: () =>
                      setSections((prev) =>
                        withRecommendedPlacement(catalog, prev, catalogIds),
                      ),
                  },
                  {
                    label: "Add available sections",
                    detail: "Only those that produce something for this run",
                    disabled: producible.every((id) => sections.includes(id)),
                    // Two different reasons to be unavailable, and "they are
                    // all in the report already" would be a claim the app
                    // cannot make before anything has been probed.
                    disabledReason:
                      availability.length === 0
                        ? "Run a simulation to see which sections have results"
                        : "Every section with results is already in the report",
                    onSelect: () =>
                      setSections((prev) =>
                        withRecommendedPlacement(catalog, prev, producible),
                      ),
                  },
                  {
                    label: "Remove sections with no results",
                    detail:
                      barren.length > 0
                        ? `${barren.length} cannot render for this run`
                        : undefined,
                    // Caution, not destructive: it only drops sections the
                    // outline already marks as producing nothing, so nothing
                    // that would have appeared in the report is lost.
                    warning: true,
                    disabled: barren.length === 0,
                    // Two different reasons to be unavailable, and saying
                    // "every section produces results" when nothing has been
                    // probed would be a claim the app cannot make.
                    disabledReason:
                      availability.length === 0
                        ? "Run a simulation to see which sections have results"
                        : "Every section produces results for this run",
                    onSelect: () =>
                      setSections((prev) =>
                        prev.filter((id) => !barren.includes(id)),
                      ),
                  },
                  {
                    label: "Clear all settings",
                    detail: "Discards every heading and option",
                    danger: true,
                    disabled: customisedCount === 0,
                    disabledReason: "No section has custom settings",
                    onSelect: clearCustomisations,
                  },
                  {
                    label: "Remove all sections",
                    danger: true,
                    disabled: sections.length === 0,
                    disabledReason: "The report has no sections",
                    onSelect: () => setSections([]),
                  },
                  {
                    // Named for the fact that it discards, which is what
                    // separates it from everything above: each of those does
                    // exactly one of membership, order or settings, and this
                    // does all three. Naming it after the sections it restores
                    // made it echo "Add every section", whose whole point is
                    // that it keeps the arrangement and settings this wipes.
                    //
                    // Deliberately NOT filtered by what the run can produce:
                    // availability is empty until a run is probed, so a
                    // run-aware reset would empty the report on exactly the
                    // project most likely to reach for it.
                    label: "Reset the whole report",
                    detail: "Every section, recommended order, no settings",
                    danger: true,
                    disabled: atEverySection,
                    disabledReason:
                      "The report already holds every section, unmodified",
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
              // `scroll`, not `auto`, so the scrollbar's track is always laid
              // out and its width is always reserved: expanding a section far
              // enough to overflow would otherwise narrow every row by 5px and
              // widen them again on collapse.
              //
              // NOT `scrollbar-gutter: stable`, which looks like the property
              // for this and does nothing here — app.css styles
              // ::-webkit-scrollbar, and WebKit honours the gutter only for
              // its native scrollbar, never a custom one.
              //
              // Costs nothing visually: app.css leaves the track transparent
              // and draws a thumb only when there is something to scroll, so a
              // reserved-but-unused track is invisible.
              overflowY: "scroll",
              // Reserving the track costs 5px of the list's own width, which
              // pulled every row left of the title input, the Add palette and
              // the header buttons above it. Widen the box by exactly that
              // much so the track sits in the rail's padding and the rows line
              // up with the rest of the rail again. Insetting those three
              // instead would fix the symptom and oblige every future sibling
              // to remember the same 5px.
              marginRight: "calc(-1 * var(--scrollbar-w))",
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
              onReveal={(id) => revealSection(sections.indexOf(id))}
              revealBlocked={revealBlocked}
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
            // --accent-fg, never white: the dark theme's accent is
            // near-white, and white-on-accent is exactly the pairing the
            // token exists to prevent (app.css).
            color: exporting ? "var(--text-tertiary)" : "var(--accent-fg)",
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
                  background: active ? "var(--accent-dim)" : "transparent",
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
                  ref={htmlFrameRef}
                  title="Report preview"
                  // ─────────────────────────────────────────────────────────
                  // NEVER add `allow-scripts` to this list.
                  //
                  // `allow-same-origin` is here so the app can read and
                  // restore this frame's scroll position, which switching
                  // scenarios would otherwise reset — there is no way to
                  // reach a frame in an opaque origin. Every other
                  // restriction still applies: no forms, no popups, no
                  // top-level navigation, no plugins.
                  //
                  // Granting scripts ALONGSIDE same-origin is what defeats a
                  // sandbox: the framed document could then reach the parent
                  // and remove this attribute outright. Either token alone is
                  // safe; the pair is not.
                  //
                  // The content is first-party — hydra-report's renderer,
                  // with every model-derived string escaped — and its
                  // `is_self_contained_and_scriptless` test asserts the
                  // output carries no <script, no src=, and no external URL.
                  // That test is what keeps this safe; it is not incidental.
                  // ─────────────────────────────────────────────────────────
                  sandbox="allow-same-origin"
                  srcDoc={previewContent}
                  onLoad={restoreHtmlScroll}
                  style={{ flex: 1, border: "none", background: "#ffffff" }}
                />
              ) : format === "csv" ? (
                <CsvPreview ref={csvRef} content={previewContent} />
              ) : (
                <pre
                  ref={txtRef}
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
