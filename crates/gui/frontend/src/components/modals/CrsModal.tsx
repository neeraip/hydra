import { XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import {
  COMMON_CRS,
  normalizeEpsgCode,
  registerCustomCrsDefinitions,
  validateCustomCrsDefinition,
} from "../../canvas/coords";
import {
  deleteCustomCrsDef,
  listCustomCrsDefs,
  type CustomCrsDef,
  updateProjectCrs,
  upsertCustomCrsDef,
} from "../../hooks";

export function CrsModal() {
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
  const [projectSaving, setProjectSaving] = useState(false);

  function dismissModal() {
    setDraftCrs(project?.sourceCrs ?? "");
    setQuery("");
    setCustomCode("");
    setCustomName("");
    setCustomProj4("");
    closeCrsModal();
  }

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
    setDraftCrs(project?.sourceCrs ?? "");
    setCustomCode("");
    setCustomName("");
    setCustomProj4("");
    return () => {
      cancelled = true;
    };
  }, [crsModalOpen]);

  useEffect(() => {
    if (!crsModalOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismissModal();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [crsModalOpen, closeCrsModal, project?.sourceCrs]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const merged = [
      ...savedCustom.map((c) => ({
        label: `${c.label} (${c.epsg})`,
        epsg: c.epsg,
        custom: true,
      })),
      ...COMMON_CRS.map((c) => ({ ...c, custom: false })),
    ];

    if (!q) return merged;
    return merged.filter((c) => {
      const hay = `${c.label} ${c.epsg}`.toLowerCase();
      return hay.includes(q);
    });
  }, [query, savedCustom]);

  const normalizedSaved = normalizeEpsgCode(project?.sourceCrs ?? "");
  const normalizedDraft = normalizeEpsgCode(draftCrs);
  const dirty = normalizedDraft !== normalizedSaved;

  function selectDraft(raw: string) {
    const code = normalizeEpsgCode(raw);
    if (!code) {
      showToast("Enter a CRS code.", "warn");
      return;
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
          width: "min(700px, 90vw)",
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
            height: 52,
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
            Saved: {project.sourceCrs}
          </span>
          {dirty && (
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
            onChange={(e) => setQuery(e.currentTarget.value)}
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
            }}
          >
            {filtered.length} match{filtered.length === 1 ? "" : "es"}
          </div>
        </div>

        <div
          style={{
            overflowY: "auto",
            display: "flex",
            flexDirection: "column",
            minHeight: 180,
            maxHeight: 360,
          }}
        >
          {filtered.map((c) => {
            const active = c.epsg === project.sourceCrs;
            return (
              <button
                type="button"
                key={c.epsg}
                onClick={() => selectDraft(c.epsg)}
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
                      : "transparent",
                  color:
                    normalizedDraft === c.epsg
                      ? "var(--accent)"
                      : "var(--text-secondary)",
                  textAlign: "left",
                  padding: "9px 12px",
                  cursor: projectSaving ? "wait" : "pointer",
                  fontFamily: "var(--font-ui)",
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
          {filtered.length === 0 && (
            <div
              style={{
                padding: "16px 12px",
                fontSize: 13,
                color: "var(--text-tertiary)",
                borderBottom: "1px solid var(--border)",
              }}
            >
              No curated CRS matched your search.
            </div>
          )}
        </div>

        <div
          style={{
            padding: 12,
            borderTop: "1px solid var(--border)",
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
            }}
          >
            <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
              Save a reusable custom projection (available in all projects).
            </div>
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
              rows={3}
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
                }}
              >
                {savedCustom.map((c, idx) => (
                  <div
                    key={c.epsg}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      padding: "6px 8px",
                      borderTop: idx === 0 ? "none" : "1px solid var(--border)",
                    }}
                  >
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div
                        style={{
                          fontSize: 12,
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
                          fontSize: 11,
                          color: "var(--text-tertiary)",
                          fontFamily: "var(--font-mono)",
                        }}
                      >
                        {c.epsg}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => selectDraft(c.epsg)}
                      className="tool-btn"
                      style={{
                        width: "auto",
                        height: 24,
                        padding: "0 8px",
                        fontSize: 11,
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
                        fontSize: 11,
                        padding: 0,
                      }}
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            )}
            <div
              style={{
                marginTop: 6,
                paddingTop: 10,
                borderTop: "1px solid var(--border)",
                display: "flex",
                justifyContent: "flex-end",
                gap: 8,
              }}
            >
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
        </div>
      </div>
    </div>
  );
}
