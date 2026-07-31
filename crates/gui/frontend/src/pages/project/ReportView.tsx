import {
  ArrowDownIcon,
  ArrowUpIcon,
  Cog6ToothIcon,
  DocumentArrowDownIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import { ACCENT } from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import {
  builderStateFromTemplate,
  buildTemplateJson,
  exportReport,
  generateReport,
  getReportBlockOptions,
  getReportTemplate,
  listReportBlocks,
  type ReportBlockInfo,
  type ReportFormat,
  type ReportOptionInfo,
  saveReportTemplate,
} from "../../hooks/reports";
import { BlockOptions, type OptionValues } from "./ReportView/BlockOptions";

const PREVIEW_DEBOUNCE_MS = 350;
const SAVE_DEBOUNCE_MS = 800;

const FORMATS: { id: ReportFormat; label: string }[] = [
  { id: "html", label: "HTML" },
  { id: "pdf", label: "PDF" },
  { id: "txt", label: "Text" },
  { id: "csv", label: "CSV" },
];

/**
 * Report view: template builder (left rail) + per-format document preview.
 *
 * The report target is the app's ACTIVE scenario (the scenario strip in
 * the project toolbar is the one selection mechanism — this view adds no
 * second picker); the document's provenance line names it. The rail's
 * state — title, block order, enabled set — IS the template; it
 * round-trips through the same JSON format `hydra report --template`
 * consumes, persisted per project. The preview shows the SELECTED format
 * exactly as it exports (html rendered in a sandboxed frame; txt/csv as
 * the literal bytes), without a timestamp so re-renders are stable;
 * the single Export button stamps the generation time and saves via the
 * OS dialog in the selected format.
 */
export function ReportView() {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();

  const [catalog, setCatalog] = useState<ReportBlockInfo[]>([]);
  const [title, setTitle] = useState("Simulation Report");
  const [order, setOrder] = useState<string[]>([]);
  const [enabled, setEnabled] = useState<Set<string>>(new Set());
  // Per-block options. Held opaquely so a key this build cannot render — a
  // hand-authored one, or one from a newer engine — survives the round-trip
  // even though the editor below only shows described keys.
  const [optionsById, setOptionsById] = useState<Record<string, unknown>>({});
  // Which block's options are open. One at a time: the rail is narrow and
  // the forms are the only thing in it that scrolls.
  const [openOptionsFor, setOpenOptionsFor] = useState<string | null>(null);
  // Engine-described options per block id, resolved for the active target.
  const [descriptorsById, setDescriptorsById] = useState<
    Record<string, ReportOptionInfo[]>
  >({});
  const [format, setFormat] = useState<ReportFormat>("html");
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
        setOrder(restored.order);
        setEnabled(restored.enabled);
        setOptionsById(restored.optionsById);
      } else {
        setOrder(blocks.map((b) => b.id));
        setEnabled(new Set(blocks.map((b) => b.id)));
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

  const templateJson = useMemo(
    () => buildTemplateJson(title, order, enabled, optionsById),
    [title, order, enabled, optionsById],
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
  function toggle(id: string) {
    setEnabled((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  function move(id: string, delta: -1 | 1) {
    setOrder((prev) => {
      const index = prev.indexOf(id);
      const target = index + delta;
      if (index < 0 || target < 0 || target >= prev.length) return prev;
      const next = [...prev];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });
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
          flex: "0 0 280px",
          borderRight: "1px solid var(--border)",
          background: "var(--bg-panel)",
          display: "flex",
          flexDirection: "column",
          overflow: "auto",
          padding: "16px 14px",
          gap: 16,
        }}
      >
        <div>
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

        <div style={{ flex: 1 }}>
          <FieldLabel>Sections</FieldLabel>
          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
            {order.map((id, index) => {
              const block = blockById.get(id);
              if (!block) return null;
              const checked = enabled.has(id);
              const descriptors = descriptorsById[id] ?? [];
              const open = openOptionsFor === id;
              const configured =
                Object.keys(
                  (optionsById[id] as Record<string, unknown> | undefined) ??
                    {},
                ).length > 0;
              return (
                <div key={id}>
                  <div
                    title={block.summary}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      padding: "6px 8px",
                      borderRadius: 6,
                      background: checked
                        ? "var(--bg-elevated)"
                        : "transparent",
                      border: "1px solid",
                      borderColor: checked ? "var(--border)" : "transparent",
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggle(id)}
                      style={{ accentColor: ACCENT, flexShrink: 0 }}
                    />
                    <span
                      style={{
                        flex: 1,
                        fontSize: "var(--text-lg)",
                        color: checked
                          ? "var(--text-primary)"
                          : "var(--text-tertiary)",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {block.title}
                    </span>
                    {descriptors.length > 0 ? (
                      <RowButton
                        label={open ? "Hide settings" : "Settings"}
                        active={open || configured}
                        onClick={() => setOpenOptionsFor(open ? null : id)}
                      >
                        <Cog6ToothIcon style={{ width: 12, height: 12 }} />
                      </RowButton>
                    ) : null}
                    <RowButton
                      label="Move up"
                      disabled={index === 0}
                      onClick={() => move(id, -1)}
                    >
                      <ArrowUpIcon style={{ width: 12, height: 12 }} />
                    </RowButton>
                    <RowButton
                      label="Move down"
                      disabled={index === order.length - 1}
                      onClick={() => move(id, 1)}
                    >
                      <ArrowDownIcon style={{ width: 12, height: 12 }} />
                    </RowButton>
                  </div>
                  {open ? (
                    <BlockOptions
                      descriptors={descriptors}
                      values={optionsById[id] as OptionValues}
                      onChange={(next) =>
                        setOptionsById((prev) => {
                          const updated = { ...prev };
                          if (next === undefined) delete updated[id];
                          else updated[id] = next;
                          return updated;
                        })
                      }
                    />
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>

        <button
          type="button"
          disabled={exporting}
          onClick={handleExport}
          style={{
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
                onClick={() => setFormat(f.id)}
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
               the app theme (the artifact itself is theme-less). */
            <div
              style={{
                flex: 1,
                minWidth: 0,
                maxWidth: 860,
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

function RowButton({
  label,
  disabled = false,
  active = false,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  /** Tints the button — used to show a block carries non-default settings
   * even while its form is collapsed. */
  active?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: 20,
        height: 20,
        padding: 0,
        borderRadius: 4,
        border: "none",
        background: "transparent",
        color: disabled
          ? "var(--text-tertiary)"
          : active
            ? ACCENT
            : "var(--text-secondary)",
        cursor: disabled ? "default" : "pointer",
        opacity: disabled ? 0.4 : 1,
      }}
    >
      {children}
    </button>
  );
}
