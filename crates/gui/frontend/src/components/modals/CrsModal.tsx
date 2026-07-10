import { XMarkIcon } from "@heroicons/react/16/solid";
import { useCallback, useEffect, useRef, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import {
  normalizeEpsgCode,
  registerCustomCrsDefinitions,
  validateCustomCrsDefinition,
} from "../../canvas/coords";
import {
  type CrsCatalogEntry,
  type CrsCatalogPage,
  type CustomCrsDef,
  deleteCustomCrsDef,
  listCrsCatalogPage,
  listCustomCrsDefs,
  updateProjectCrs,
  upsertCustomCrsDef,
} from "../../hooks";

export function CrsModal() {
  const PAGE_SIZE = 80;
  const {
    activeProjectId,
    bumpProjects,
    closeCrsModal,
    crsModalOpen,
    showToast,
  } = useAppState();
  const { project } = useActiveProject();

  const [query, setQuery] = useState("");
  const [draftCrs, setDraftCrs] = useState("");
  const [customCode, setCustomCode] = useState("");
  const [customName, setCustomName] = useState("");
  const [customProj4, setCustomProj4] = useState("");
  const [savedCustom, setSavedCustom] = useState<CustomCrsDef[]>([]);
  const [catalogPage, setCatalogPage] = useState<CrsCatalogPage>({
    items: [],
    total: 0,
    page: 0,
    pageSize: PAGE_SIZE,
    hasMore: false,
  });
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [pageIndex, setPageIndex] = useState(0);
  const [catalogVersion, setCatalogVersion] = useState(0);
  const [selectedResultIndex, setSelectedResultIndex] = useState(0);
  const [panelView, setPanelView] = useState<"select" | "custom">("select");
  const [projectSaving, setProjectSaving] = useState(false);
  const resultButtonRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const dismissModal = useCallback(() => {
    setDraftCrs(project?.sourceCrs ?? "");
    setQuery("");
    setPageIndex(0);
    setSelectedResultIndex(0);
    setPanelView("select");
    setCustomCode("");
    setCustomName("");
    setCustomProj4("");
    closeCrsModal();
  }, [closeCrsModal, project?.sourceCrs]);

  useEffect(() => {
    if (!crsModalOpen) return;
    let cancelled = false;
    void (async () => {
      const defs = await listCustomCrsDefs();
      if (cancelled) return;
      setSavedCustom(defs);
      registerCustomCrsDefinitions(defs);
    })();
    setQuery("");
    setPageIndex(0);
    setSelectedResultIndex(0);
    setPanelView("select");
    setDraftCrs(project?.sourceCrs ?? "");
    setCustomCode("");
    setCustomName("");
    setCustomProj4("");
    return () => {
      cancelled = true;
    };
  }, [crsModalOpen, project?.sourceCrs]);

  useEffect(() => {
    if (!crsModalOpen || panelView !== "select") return;
    let cancelled = false;
    setCatalogLoading(true);
    void (async () => {
      const page = await listCrsCatalogPage({
        query,
        page: pageIndex,
        pageSize: PAGE_SIZE,
      });
      if (cancelled) return;
      setCatalogPage(page);
      setCatalogLoading(false);
    })();
    return () => {
      cancelled = true;
    };
  }, [crsModalOpen, panelView, query, pageIndex, catalogVersion]);

  useEffect(() => {
    if (panelView !== "select") return;
    const draft = normalizeEpsgCode(draftCrs);
    const draftIdx = catalogPage.items.findIndex((entry) => entry.epsg === draft);
    setSelectedResultIndex((prev) => {
      if (draftIdx >= 0) return draftIdx;
      if (catalogPage.items.length === 0) return 0;
      return Math.min(prev, catalogPage.items.length - 1);
    });
  }, [panelView, catalogPage.items, draftCrs]);

  useEffect(() => {
    if (panelView !== "select") return;
    const el = resultButtonRefs.current[selectedResultIndex];
    if (el) {
      el.scrollIntoView({ block: "nearest" });
    }
  }, [panelView, selectedResultIndex, catalogPage.items]);

  useEffect(() => {
    if (!crsModalOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismissModal();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [crsModalOpen, dismissModal]);

  const normalizedSaved = normalizeEpsgCode(project?.sourceCrs ?? "");
  const normalizedDraft = normalizeEpsgCode(draftCrs);
  const dirty = normalizedDraft !== normalizedSaved;
  const highlightedEntry = catalogPage.items[selectedResultIndex] ?? null;

  function selectDraft(value: string | CrsCatalogEntry) {
    const raw = typeof value === "string" ? value : value.epsg;
    const code = normalizeEpsgCode(raw);
    if (!code) {
      showToast("Enter a CRS code.", "warn");
      return;
    }
    if (typeof value !== "string" && value.proj4?.trim()) {
      registerCustomCrsDefinitions([
        { label: value.label, epsg: code, proj4: value.proj4 },
      ]);
    }
    setDraftCrs(code);
  }

  async function saveProjectCrs() {
    if (!activeProjectId || !project) return;

    if (!normalizedDraft) {
      showToast("Select a CRS code before saving.", "warn");
      return;
    }

    if (!dirty) {
      closeCrsModal();
      return;
    }

    setProjectSaving(true);
    try {
      const ok = await updateProjectCrs(activeProjectId, normalizedDraft);
      if (!ok) {
        showToast("Could not update source CRS.", "error");
        return;
      }
      bumpProjects();
      showToast(`Source CRS updated to ${normalizedDraft}.`, "success");
      closeCrsModal();
    } finally {
      setProjectSaving(false);
    }
  }

  useEffect(() => {
    if (!crsModalOpen || panelView !== "select") return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key !== "Enter") return;
      if (projectSaving || !dirty) return;
      e.preventDefault();
      void saveProjectCrs();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [crsModalOpen, panelView, projectSaving, dirty, saveProjectCrs]);

  function saveReusableCustom() {
    if (!customName.trim()) {
      showToast("Custom CRS name is required.", "error");
      return;
    }
    const epsg = normalizeEpsgCode(customCode);
    if (!epsg) {
      showToast("CRS code is required.", "error");
      return;
    }
    if (!validateCustomCrsDefinition(epsg, customProj4)) {
      showToast("Invalid proj4 definition for this EPSG code.", "error");
      return;
    }

    void (async () => {
      const defs = await upsertCustomCrsDef({
        label: customName,
        epsg,
        proj4: customProj4,
      });
      if (!defs) {
        showToast("Could not save custom CRS.", "error");
        return;
      }
      setSavedCustom(defs);
      registerCustomCrsDefinitions(defs);
      setCatalogVersion((v) => v + 1);
      setCustomName("");
      setCustomCode("");
      setCustomProj4("");
      showToast(`Saved custom CRS ${epsg}.`, "success");
    })();
  }

  function removeSaved(epsg: string) {
    void (async () => {
      const defs = await deleteCustomCrsDef(epsg);
      if (!defs) {
        showToast("Could not remove custom CRS.", "error");
        return;
      }
      setSavedCustom(defs);
      registerCustomCrsDefinitions(defs);
      setCatalogVersion((v) => v + 1);
      showToast(`Removed custom CRS ${epsg}.`, "info");
    })();
  }

  if (!crsModalOpen || !project) return null;

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the modal on pointer interaction.
    // biome-ignore lint/a11y/useKeyWithClickEvents: backdrop closes the modal on pointer interaction.
    <div
      onClick={dismissModal}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--bg-overlay)",
        zIndex: 205,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {/* biome-ignore lint/a11y/noStaticElementInteractions: panel only stops backdrop clicks. */}
      <div
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
        style={{
          width: "min(740px, 92vw)",
          maxHeight: "min(680px, 86vh)",
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          backdropFilter: "blur(24px)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          boxShadow: "0 24px 80px rgba(0,0,0,0.5)",
        }}
      >
        <div
          style={{
            flexShrink: 0,
            minHeight: 52,
            borderBottom: "1px solid var(--border)",
            background: "var(--bg-panel)",
            display: "flex",
            alignItems: "center",
            padding: "0 16px",
            gap: 10,
          }}
        >
          <span
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Set Source CRS
          </span>
          <span
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            {panelView === "select"
              ? `Saved: ${project.sourceCrs}`
              : "Create or manage reusable custom CRS definitions"}
          </span>
          {panelView === "select" && dirty && (
            <span
              style={{
                fontSize: 11,
                color: "var(--accent)",
                fontFamily: "var(--font-ui)",
              }}
            >
              Unsaved change: {normalizedDraft || "(none)"}
            </span>
          )}
          <div
            style={{
              marginLeft: 10,
              display: "inline-flex",
              gap: 6,
              padding: 3,
              border: "1px solid var(--border)",
              borderRadius: 8,
              background: "var(--bg-input)",
            }}
          >
            <button
              type="button"
              onClick={() => setPanelView("select")}
              className="tool-btn"
              style={{
                width: "auto",
                height: 24,
                padding: "0 8px",
                fontSize: 11,
                borderRadius: 6,
                background:
                  panelView === "select" ? "var(--accent-dim)" : "transparent",
                color:
                  panelView === "select" ? "var(--accent)" : "var(--text-secondary)",
              }}
            >
              Select CRS
            </button>
            <button
              type="button"
              onClick={() => setPanelView("custom")}
              className="tool-btn"
              style={{
                width: "auto",
                height: 24,
                padding: "0 8px",
                fontSize: 11,
                borderRadius: 6,
                background:
                  panelView === "custom" ? "var(--accent-dim)" : "transparent",
                color:
                  panelView === "custom" ? "var(--accent)" : "var(--text-secondary)",
              }}
            >
              Custom CRS
            </button>
          </div>
          <div style={{ flex: 1 }} />
          <button
            type="button"
            onClick={dismissModal}
            aria-label="Close"
            style={{
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              width: 28,
              height: 28,
              border: "none",
              background: "transparent",
              color: "var(--text-secondary)",
              borderRadius: 5,
              cursor: "pointer",
              padding: 0,
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
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        {panelView === "select" ? (
          <>
            <div
              style={{
                padding: 14,
                borderBottom: "1px solid var(--border)",
                display: "grid",
                gridTemplateColumns: "1fr auto",
                gap: 8,
              }}
            >
              <input
                type="search"
                value={query}
                onChange={(e) => {
                  setQuery(e.currentTarget.value);
                  setPageIndex(0);
                }}
                onKeyDown={(e) => {
                  if (catalogPage.items.length === 0) return;
                  if (e.key === "ArrowDown") {
                    e.preventDefault();
                    setSelectedResultIndex((idx) =>
                      Math.min(catalogPage.items.length - 1, idx + 1),
                    );
                    return;
                  }
                  if (e.key === "ArrowUp") {
                    e.preventDefault();
                    setSelectedResultIndex((idx) => Math.max(0, idx - 1));
                    return;
                  }
                  if (e.key === "Enter") {
                    e.preventDefault();
                    const entry = catalogPage.items[selectedResultIndex];
                    if (entry) selectDraft(entry);
                  }
                }}
                placeholder="Search by name or EPSG code"
                style={{
                  fontSize: 13,
                  padding: "7px 10px",
                  background: "var(--bg-input, var(--bg-card))",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-ui)",
                }}
              />
              <div
                style={{
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                  display: "flex",
                  alignItems: "center",
                  whiteSpace: "nowrap",
                }}
              >
                {catalogLoading
                  ? "Loading..."
                  : `${catalogPage.total} matches · ↑↓ navigate · Enter select · Cmd/Ctrl+Enter save`}
              </div>
            </div>

            <div
              style={{
                overflowY: "auto",
                display: "flex",
                flexDirection: "column",
                minHeight: 220,
                maxHeight: 420,
              }}
            >
              {catalogPage.items.map((c, idx) => {
                const active = c.epsg === project.sourceCrs;
                const highlighted = idx === selectedResultIndex;
                return (
                  <button
                    type="button"
                    key={c.epsg}
                    ref={(el) => {
                      resultButtonRefs.current[idx] = el;
                    }}
                    onClick={() => selectDraft(c)}
                    onMouseEnter={() => setSelectedResultIndex(idx)}
                    disabled={projectSaving}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 10,
                      border: "none",
                      borderBottom: "1px solid var(--border)",
                      background:
                        normalizedDraft === c.epsg
                          ? "var(--accent-dim)"
                          : highlighted
                            ? "var(--nav-hover)"
                            : "transparent",
                      color:
                        normalizedDraft === c.epsg
                          ? "var(--accent)"
                          : "var(--text-secondary)",
                      textAlign: "left",
                      padding: "10px 12px",
                      cursor: projectSaving ? "wait" : "pointer",
                      fontFamily: "var(--font-ui)",
                      boxShadow: highlighted
                        ? "inset 2px 0 0 var(--accent)"
                        : "none",
                    }}
                  >
                    <span style={{ fontSize: 13 }}>{c.label}</span>
                    <span style={{ display: "inline-flex", gap: 6 }}>
                      {c.custom && (
                        <span
                          style={{
                            fontSize: 10,
                            fontWeight: 700,
                            letterSpacing: "0.05em",
                            color: "var(--text-tertiary)",
                          }}
                        >
                          CUSTOM
                        </span>
                      )}
                      {active && (
                        <span
                          style={{
                            fontSize: 11,
                            fontWeight: 600,
                            letterSpacing: "0.04em",
                          }}
                        >
                          SAVED
                        </span>
                      )}
                    </span>
                  </button>
                );
              })}
              {!catalogLoading && catalogPage.items.length === 0 && (
                <div
                  style={{
                    padding: "16px 12px",
                    fontSize: 13,
                    color: "var(--text-tertiary)",
                    borderBottom: "1px solid var(--border)",
                  }}
                >
                  No CRS entries matched your search.
                </div>
              )}
            </div>

            <div
              style={{
                padding: "8px 12px",
                borderTop: "1px solid var(--border)",
                borderBottom: "1px solid var(--border)",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
                Page {catalogPage.page + 1}
              </span>
              <div style={{ display: "flex", gap: 8 }}>
                <button
                  type="button"
                  className="tool-btn"
                  onClick={() => setPageIndex((p) => Math.max(0, p - 1))}
                  disabled={catalogLoading || pageIndex === 0}
                  style={{
                    width: "auto",
                    height: 24,
                    padding: "0 8px",
                    fontSize: 11,
                  }}
                >
                  Prev
                </button>
                <button
                  type="button"
                  className="tool-btn"
                  onClick={() => setPageIndex((p) => p + 1)}
                  disabled={catalogLoading || !catalogPage.hasMore}
                  style={{
                    width: "auto",
                    height: 24,
                    padding: "0 8px",
                    fontSize: 11,
                  }}
                >
                  Next
                </button>
              </div>
            </div>

            <div
              style={{
                padding: "10px 12px",
                borderTop: "1px solid var(--border)",
                background: "var(--bg-panel)",
                position: "sticky",
                bottom: 0,
                boxShadow: "0 -8px 18px rgba(0,0,0,0.18)",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 8,
              }}
            >
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 4,
                }}
              >
                <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
                  Selected CRS
                </span>
                <span
                  style={{
                    fontSize: 12,
                    color: "var(--text-primary)",
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  {normalizedDraft || "(none)"}
                </span>
                {highlightedEntry && (
                  <span
                    style={{
                      fontSize: 10,
                      color: "var(--text-tertiary)",
                      fontFamily: "var(--font-ui)",
                    }}
                  >
                    Highlighted: {highlightedEntry.epsg}
                  </span>
                )}
              </div>
              <div style={{ display: "flex", gap: 8 }}>
                <button
                  type="button"
                  className="tool-btn"
                  onClick={() => setPanelView("custom")}
                  style={{
                    width: "auto",
                    height: 28,
                    padding: "0 10px",
                    fontSize: 12,
                  }}
                >
                  Manage custom CRS
                </button>
                <button
                  type="button"
                  onClick={dismissModal}
                  className="tool-btn"
                  style={{
                    width: "auto",
                    height: 28,
                    padding: "0 10px",
                    fontSize: 12,
                  }}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={() => void saveProjectCrs()}
                  className="tool-btn"
                  disabled={projectSaving || !dirty}
                  style={{
                    width: "auto",
                    height: 28,
                    padding: "0 10px",
                    fontSize: 12,
                  }}
                >
                  {projectSaving ? "Saving..." : "Save CRS"}
                </button>
              </div>
            </div>
          </>
        ) : (
          <>
            <div
              style={{
                padding: 14,
                borderBottom: "1px solid var(--border)",
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 8,
              }}
            >
              <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
                Save a reusable CRS projection available to all projects.
              </span>
              <button
                type="button"
                className="tool-btn"
                onClick={() => setPanelView("select")}
                style={{
                  width: "auto",
                  height: 26,
                  padding: "0 8px",
                  fontSize: 11,
                }}
              >
                Back to picker
              </button>
            </div>

            <div
              style={{
                overflowY: "auto",
                display: "flex",
                flexDirection: "column",
                gap: 10,
                padding: 12,
                minHeight: 240,
                maxHeight: 480,
              }}
            >
              <input
                type="text"
                value={customName}
                onChange={(e) => setCustomName(e.currentTarget.value)}
                placeholder="Display name (e.g. Utility Grid Local)"
                style={{
                  fontSize: 13,
                  padding: "7px 10px",
                  background: "var(--bg-input, var(--bg-card))",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-ui)",
                }}
              />
              <input
                type="text"
                value={customCode}
                onChange={(e) => setCustomCode(e.currentTarget.value)}
                placeholder="CRS code (e.g. EPSG:28355 or LOCAL:MYGRID)"
                style={{
                  fontSize: 13,
                  padding: "7px 10px",
                  background: "var(--bg-input, var(--bg-card))",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-mono)",
                }}
              />
              <textarea
                value={customProj4}
                onChange={(e) => setCustomProj4(e.currentTarget.value)}
                placeholder="Proj4 definition (e.g. +proj=tmerc +lat_0=... )"
                rows={4}
                style={{
                  fontSize: 12,
                  padding: "7px 10px",
                  background: "var(--bg-input, var(--bg-card))",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-mono)",
                  resize: "vertical",
                }}
              />

              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <button
                  type="button"
                  className="tool-btn"
                  onClick={saveReusableCustom}
                  style={{
                    width: "auto",
                    height: 28,
                    padding: "0 10px",
                    fontSize: 12,
                  }}
                >
                  Save custom CRS
                </button>
                {savedCustom.length > 0 && (
                  <div
                    style={{
                      fontSize: 11,
                      color: "var(--text-tertiary)",
                      display: "flex",
                      alignItems: "center",
                    }}
                  >
                    {savedCustom.length} saved
                  </div>
                )}
              </div>

              {savedCustom.length > 0 && (
                <div
                  style={{
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    overflow: "hidden",
                    maxHeight: 190,
                    overflowY: "auto",
                  }}
                >
                  {savedCustom.map((c, idx) => (
                    <div
                      key={c.epsg}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 6,
                        padding: "4px 8px",
                        borderTop: idx === 0 ? "none" : "1px solid var(--border)",
                      }}
                    >
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div
                          style={{
                            fontSize: 11,
                            color: "var(--text-primary)",
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                        >
                          {c.label}
                        </div>
                        <div
                          style={{
                            fontSize: 10,
                            color: "var(--text-tertiary)",
                            fontFamily: "var(--font-mono)",
                          }}
                        >
                          {c.epsg}
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={() => {
                          selectDraft(c.epsg);
                          setPanelView("select");
                        }}
                        className="tool-btn"
                        style={{
                          width: "auto",
                          height: 22,
                          padding: "0 7px",
                          fontSize: 10,
                        }}
                      >
                        Select
                      </button>
                      <button
                        type="button"
                        onClick={() => removeSaved(c.epsg)}
                        style={{
                          border: "none",
                          background: "transparent",
                          color: "var(--status-error)",
                          cursor: "pointer",
                          fontSize: 10,
                          padding: 0,
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
