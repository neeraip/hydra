/**
 * Choosing the coordinate system a model's coordinates are in.
 *
 * The old picker asked the question the way a database would: here are
 * 8,351 codes, which one. That only works for someone who already knows
 * the answer, and the people who know the answer are not the ones who need
 * a picker.
 *
 * This one is built around the two facts that are actually available:
 *
 *   the model's own numbers    They rule things out before anything is
 *                              chosen — coordinates in the hundreds of
 *                              thousands are not longitude and latitude,
 *                              and that is worth saying out loud.
 *   where a candidate lands    Apply a system and the network appears
 *                              somewhere on earth. An engineer who cannot
 *                              recall an EPSG code knows at a glance
 *                              whether their network belongs in Leeds or
 *                              in the Gulf of Guinea. This is the check
 *                              that actually prevents a wrong answer, and
 *                              the old design had nothing like it.
 *
 * Everything else follows from making those first-class. There is one list
 * and one way to choose — a local grid is a row in it, because "none of
 * these" is an answer and answers are rows. Defining a system is a
 * disclosure rather than a tab, because it is a different kind of act:
 * what it produces comes back as an ordinary row.
 */

import { XMarkIcon } from "@heroicons/react/16/solid";
import proj4 from "proj4";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import {
  COMMON_CRS,
  ensureEpsgDef,
  LOCAL_CRS,
  normalizeEpsgCode,
  registerCustomCrsDefinitions,
  validateCustomCrsDefinition,
} from "../../canvas/coords";
import {
  betterUtmZone,
  type CoordinateReading,
  plausibleLatLon,
  readCoordinates,
} from "../../canvas/crsInference";
import {
  type CrsCatalogEntry,
  type CustomCrsDef,
  deleteCustomCrsDef,
  listCrsCatalogPage,
  listCustomCrsDefs,
  updateProjectCrs,
  upsertCustomCrsDef,
  useNodes,
} from "../../hooks";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

/** Enough rows to scroll through, few enough to stay a shortlist. Searching
 * narrows; nobody pages through a coordinate catalogue. */
const RESULT_LIMIT = 60;

/** One selectable answer. `LOCAL` is one of these, deliberately: it is a
 * thing the model can be, not a mode the picker can be in. */
interface Choice {
  code: string;
  label: string;
  /** Shown under the label — what this answer means, or where it puts you. */
  detail?: string;
  custom?: boolean;
}

/** Where a candidate system puts the model's centre. */
interface Landing {
  lat: number;
  lon: number;
  plausible: boolean;
}

function landingFor(
  code: string,
  reading: CoordinateReading | null,
): Landing | null {
  if (!reading || code === LOCAL_CRS) return null;
  if (code === "EPSG:4326") {
    const [lon, lat] = reading.centre;
    return { lat, lon, plausible: plausibleLatLon(lat, lon) };
  }
  if (!ensureEpsgDef(code)) return null;
  try {
    const [lon, lat] = proj4(code, "EPSG:4326", reading.centre);
    return { lat, lon, plausible: plausibleLatLon(lat, lon) };
  } catch {
    return null;
  }
}

function formatLatLon(lat: number, lon: number): string {
  const ns = lat >= 0 ? "N" : "S";
  const ew = lon >= 0 ? "E" : "W";
  return `${Math.abs(lat).toFixed(4)}° ${ns}, ${Math.abs(lon).toFixed(4)}° ${ew}`;
}

function formatExtent(reading: CoordinateReading): string {
  const round = (v: number) =>
    Math.abs(v) >= 1000 ? Math.round(v).toLocaleString() : v.toFixed(4);
  return `${round(reading.minX)} → ${round(reading.maxX)} across, ${round(reading.minY)} → ${round(reading.maxY)} up`;
}

export function CrsPicker() {
  const {
    activeProjectId,
    bumpProjects,
    closeCrsModal,
    crsModalOpen,
    showToast,
  } = useAppState();
  const { project } = useActiveProject();
  const nodes = useNodes();

  const [query, setQuery] = useState("");
  const [draft, setDraft] = useState("");
  const [results, setResults] = useState<CrsCatalogEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [defining, setDefining] = useState(false);
  const [defCode, setDefCode] = useState("");
  const [defLabel, setDefLabel] = useState("");
  const [defBody, setDefBody] = useState("");
  const [defReusable, setDefReusable] = useState(true);
  const [customDefs, setCustomDefs] = useState<CustomCrsDef[]>([]);

  const saved = normalizeEpsgCode(project?.sourceCrs ?? "");
  const normalized = normalizeEpsgCode(draft);
  const dirty = normalized !== saved;

  // What the coordinates themselves say, before any system is chosen.
  const reading = useMemo(() => readCoordinates(nodes), [nodes]);

  const dismiss = useCallback(() => {
    setQuery("");
    setDefining(false);
    setDefCode("");
    setDefLabel("");
    setDefBody("");
    closeCrsModal();
  }, [closeCrsModal]);

  useEffect(() => {
    if (!crsModalOpen) return;
    setDraft(project?.sourceCrs ?? "");
    setQuery("");
    setDefining(false);
    let cancelled = false;
    void (async () => {
      const defs = await listCustomCrsDefs();
      if (cancelled) return;
      setCustomDefs(defs);
      registerCustomCrsDefinitions(defs);
    })();
    return () => {
      cancelled = true;
    };
  }, [crsModalOpen, project?.sourceCrs]);

  useEffect(() => {
    if (!crsModalOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [crsModalOpen, dismiss]);

  // Catalogue search. Only ever runs with a query: an alphabetical page one
  // of every system on earth is not a starting point, it is noise that
  // implies browsing might work.
  useEffect(() => {
    if (!crsModalOpen) return;
    const q = query.trim();
    if (!q) {
      setResults([]);
      setTotal(0);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void (async () => {
      try {
        const page = await listCrsCatalogPage({
          query: q,
          page: 0,
          pageSize: RESULT_LIMIT,
        });
        if (!cancelled) {
          setResults(page.items);
          setTotal(page.total);
        }
      } catch {
        if (!cancelled) {
          setResults([]);
          setTotal(0);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [crsModalOpen, query]);

  /** The shortlist shown before anyone searches: what this project already
   * uses, what the user has defined, and the systems most models are in. */
  const suggestions = useMemo<Choice[]>(() => {
    const seen = new Set<string>([LOCAL_CRS]);
    const out: Choice[] = [];
    const push = (c: Choice) => {
      if (seen.has(c.code)) return;
      seen.add(c.code);
      out.push(c);
    };
    if (saved && saved !== LOCAL_CRS) {
      push({
        code: saved,
        label: saved,
        detail: "This project's current setting",
      });
    }
    for (const d of customDefs) {
      push({ code: normalizeEpsgCode(d.epsg), label: d.label, custom: true });
    }
    for (const c of COMMON_CRS) push({ code: c.epsg, label: c.label });
    return out;
  }, [saved, customDefs]);

  const searchChoices = useMemo<Choice[]>(
    () =>
      results.map((r) => ({
        code: normalizeEpsgCode(r.epsg),
        label: r.label,
        custom: r.custom,
      })),
    [results],
  );

  const searching = query.trim().length > 0;
  const listed = searching ? searchChoices : suggestions;

  // Where the chosen system puts the network. The whole point of the
  // picker: a code is unverifiable, a position is not.
  const landing = useMemo(
    () => landingFor(normalized, reading),
    [normalized, reading],
  );
  const zoneHint = useMemo(
    () => (landing ? betterUtmZone(normalized, landing.lon) : null),
    [landing, normalized],
  );

  function choose(code: string, entry?: CrsCatalogEntry) {
    if (entry?.proj4?.trim()) {
      registerCustomCrsDefinitions([
        {
          label: entry.label,
          epsg: normalizeEpsgCode(entry.epsg),
          proj4: entry.proj4,
        },
      ]);
    }
    setDraft(code);
  }

  async function save() {
    if (!activeProjectId || !normalized) return;
    if (!dirty) {
      dismiss();
      return;
    }
    setSaving(true);
    try {
      const ok = await updateProjectCrs(activeProjectId, normalized);
      if (!ok) {
        showToast("Could not update the coordinate system.", "error");
        return;
      }
      bumpProjects();
      showToast(
        normalized === LOCAL_CRS
          ? "Coordinates are now read as a local grid."
          : `Coordinate system set to ${normalized}.`,
        "success",
      );
      dismiss();
    } finally {
      setSaving(false);
    }
  }

  async function saveDefinition() {
    const code = normalizeEpsgCode(defCode);
    const body = defBody.trim();
    if (!code || !body) {
      showToast("A code and a definition are both needed.", "warn");
      return;
    }
    if (!validateCustomCrsDefinition(code, body)) {
      showToast("That definition could not be understood.", "error");
      return;
    }
    const def: CustomCrsDef = {
      label: defLabel.trim() || code,
      epsg: code,
      proj4: body,
    };
    registerCustomCrsDefinitions([def]);
    if (defReusable) {
      const ok = await upsertCustomCrsDef(def);
      if (ok) setCustomDefs(await listCustomCrsDefs());
    }
    setDraft(code);
    setDefining(false);
    setDefCode("");
    setDefLabel("");
    setDefBody("");
  }

  if (!crsModalOpen || !project) return null;

  const rowStyle = (code: string): React.CSSProperties => ({
    display: "flex",
    flexDirection: "column",
    gap: 2,
    alignItems: "flex-start",
    width: "100%",
    border: "none",
    borderBottom: "1px solid var(--border)",
    background: normalized === code ? "var(--selection-bg)" : "transparent",
    color: normalized === code ? "var(--accent)" : "var(--text-primary)",
    textAlign: "left",
    padding: "9px 14px",
    cursor: saving ? "wait" : "pointer",
    fontFamily: "var(--font-ui)",
  });

  return (
    <ModalBackdrop onDismiss={dismiss} zIndex={70}>
      <div
        {...stopBackdropEvents}
        style={{
          width: "min(680px, 92vw)",
          maxHeight: "min(720px, 88vh)",
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
        {/* ── Header ─────────────────────────────────────────────────── */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "12px 14px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <span
            style={{
              fontSize: "var(--text-lg)",
              fontWeight: 600,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Coordinate system
          </span>
          <span
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-tertiary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            {saved === LOCAL_CRS ? "Local grid" : saved || "Not set"}
          </span>
          <div style={{ flex: 1 }} />
          <button
            type="button"
            onClick={dismiss}
            aria-label="Close"
            style={{
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              width: 26,
              height: 26,
              border: "none",
              background: "transparent",
              color: "var(--text-secondary)",
              borderRadius: 5,
              cursor: "pointer",
              padding: 0,
            }}
          >
            <XMarkIcon width={15} height={15} />
          </button>
        </div>

        {/* ── What the model's own numbers say ───────────────────────── */}
        {reading && (
          <div
            style={{
              padding: "10px 14px",
              borderBottom: "1px solid var(--border)",
              display: "flex",
              flexDirection: "column",
              gap: 3,
              fontFamily: "var(--font-ui)",
              background: "var(--bg-input)",
            }}
          >
            <span
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-secondary)",
              }}
            >
              {reading.projected
                ? "These coordinates are too large to be longitude and latitude, so they are measured in some projected system."
                : "These coordinates are within longitude and latitude range."}
            </span>
            <span
              style={{
                fontSize: "var(--text-sm)",
                color: "var(--text-tertiary)",
                fontFamily: "var(--font-mono)",
              }}
            >
              {formatExtent(reading)} · {reading.count.toLocaleString()} nodes
            </span>
          </div>
        )}

        {/* ── Search ─────────────────────────────────────────────────── */}
        <div
          style={{
            padding: "10px 14px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            gap: 10,
            alignItems: "center",
          }}
        >
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search by name or EPSG code"
            aria-label="Search coordinate systems"
            spellCheck={false}
            style={{
              flex: 1,
              minWidth: 0,
              fontSize: "var(--text-md)",
              padding: "7px 10px",
              background: "var(--bg-input)",
              border: "1px solid var(--border)",
              borderRadius: 6,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
            }}
          />
          <span
            style={{
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
              whiteSpace: "nowrap",
              fontFamily: "var(--font-ui)",
            }}
          >
            {loading
              ? "Searching…"
              : searching
                ? `${listed.length} of ${total.toLocaleString()}`
                : "Suggestions"}
          </span>
        </div>

        {/* ── Answers ────────────────────────────────────────────────── */}
        <div style={{ overflowY: "auto", flex: 1, minHeight: 180 }}>
          {/* "None of these" is an answer, so it is a row like any other,
              chosen the same way and committed by the same button. It is
              never filtered out: it is not a search result. */}
          <button
            type="button"
            onClick={() => setDraft(LOCAL_CRS)}
            disabled={saving}
            style={rowStyle(LOCAL_CRS)}
          >
            <span style={{ fontSize: "var(--text-md)" }}>Local grid</span>
            <span
              style={{
                fontSize: "var(--text-sm)",
                color: "var(--text-tertiary)",
              }}
            >
              Coordinates are a drawing grid with no relation to the earth.
              Drawn to scale, without a basemap.
            </span>
          </button>

          {listed.map((c) => (
            <button
              type="button"
              key={c.code}
              onClick={() =>
                choose(
                  c.code,
                  results.find((r) => normalizeEpsgCode(r.epsg) === c.code),
                )
              }
              disabled={saving}
              style={rowStyle(c.code)}
            >
              <span
                style={{
                  fontSize: "var(--text-md)",
                  display: "inline-flex",
                  gap: 6,
                  alignItems: "center",
                }}
              >
                {c.label}
                {c.custom && (
                  <span
                    data-tooltip="You defined this — not an entry from the standard catalogue"
                    style={{
                      fontSize: "var(--text-2xs)",
                      fontWeight: 700,
                      letterSpacing: "0.05em",
                      color: "var(--text-tertiary)",
                      border: "1px solid var(--border)",
                      borderRadius: 4,
                      padding: "0 4px",
                    }}
                  >
                    CUSTOM
                  </span>
                )}
              </span>
              {c.detail && (
                <span
                  style={{
                    fontSize: "var(--text-sm)",
                    color: "var(--text-tertiary)",
                  }}
                >
                  {c.detail}
                </span>
              )}
            </button>
          ))}

          {searching && !loading && listed.length === 0 && (
            <div
              style={{
                padding: 16,
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
                fontStyle: "italic",
                fontFamily: "var(--font-ui)",
              }}
            >
              Nothing matches. If you have the definition itself, paste it
              below.
            </div>
          )}
        </div>

        {/* ── Define one ─────────────────────────────────────────────── */}
        <div style={{ borderTop: "1px solid var(--border)" }}>
          <button
            type="button"
            onClick={() => setDefining((v) => !v)}
            style={{
              width: "100%",
              textAlign: "left",
              padding: "9px 14px",
              background: "transparent",
              border: "none",
              color: "var(--text-secondary)",
              fontSize: "var(--text-md)",
              fontFamily: "var(--font-ui)",
              cursor: "pointer",
            }}
          >
            {defining ? "▾" : "▸"} Not listed? Paste a definition
          </button>
          {defining && (
            <div
              style={{
                padding: "0 14px 12px",
                display: "flex",
                flexDirection: "column",
                gap: 8,
              }}
            >
              <div style={{ display: "flex", gap: 8 }}>
                <input
                  value={defCode}
                  onChange={(e) => setDefCode(e.target.value)}
                  placeholder="Code, e.g. EPSG:27700"
                  aria-label="Coordinate system code"
                  spellCheck={false}
                  style={{
                    flex: "0 0 190px",
                    fontSize: "var(--text-md)",
                    padding: "6px 9px",
                    background: "var(--bg-input)",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    color: "var(--text-primary)",
                    fontFamily: "var(--font-mono)",
                  }}
                />
                <input
                  value={defLabel}
                  onChange={(e) => setDefLabel(e.target.value)}
                  placeholder="Name (optional)"
                  aria-label="Coordinate system name"
                  style={{
                    flex: 1,
                    minWidth: 0,
                    fontSize: "var(--text-md)",
                    padding: "6px 9px",
                    background: "var(--bg-input)",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    color: "var(--text-primary)",
                    fontFamily: "var(--font-ui)",
                  }}
                />
              </div>
              <textarea
                value={defBody}
                onChange={(e) => setDefBody(e.target.value)}
                placeholder="proj4 string or WKT — whichever your source gave you"
                aria-label="Coordinate system definition"
                spellCheck={false}
                rows={3}
                style={{
                  fontSize: "var(--text-sm)",
                  padding: "7px 9px",
                  background: "var(--bg-input)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-mono)",
                  resize: "vertical",
                }}
              />
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <label
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    fontSize: "var(--text-sm)",
                    color: "var(--text-secondary)",
                    fontFamily: "var(--font-ui)",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={defReusable}
                    onChange={(e) => setDefReusable(e.target.checked)}
                  />
                  Keep for other projects
                </label>
                <div style={{ flex: 1 }} />
                <button
                  type="button"
                  className="tool-btn"
                  onClick={() => void saveDefinition()}
                  style={{ width: "auto", height: 26, padding: "0 10px" }}
                >
                  Use this definition
                </button>
              </div>
              {customDefs.length > 0 && (
                <div
                  style={{
                    display: "flex",
                    flexWrap: "wrap",
                    gap: 6,
                    paddingTop: 2,
                  }}
                >
                  {customDefs.map((d) => (
                    <span
                      key={d.epsg}
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: 6,
                        fontSize: "var(--text-xs)",
                        color: "var(--text-tertiary)",
                        border: "1px solid var(--border)",
                        borderRadius: 12,
                        padding: "2px 8px",
                        fontFamily: "var(--font-ui)",
                      }}
                    >
                      {d.label}
                      <button
                        type="button"
                        aria-label={`Forget ${d.label}`}
                        onClick={() =>
                          void deleteCustomCrsDef(d.epsg).then(async () =>
                            setCustomDefs(await listCustomCrsDefs()),
                          )
                        }
                        style={{
                          border: "none",
                          background: "transparent",
                          color: "var(--status-error)",
                          cursor: "pointer",
                          padding: 0,
                          fontSize: "var(--text-xs)",
                        }}
                      >
                        ✕
                      </button>
                    </span>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {/* ── Where this puts the network, and the commit ─────────────── */}
        <div
          style={{
            padding: "10px 14px",
            borderTop: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            gap: 12,
            background: "var(--bg-panel)",
          }}
        >
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 2,
              minWidth: 0,
              flex: 1,
              fontFamily: "var(--font-ui)",
            }}
          >
            <span
              style={{
                fontSize: "var(--text-sm)",
                color: "var(--text-tertiary)",
              }}
            >
              {normalized === LOCAL_CRS
                ? "Local grid"
                : normalized || "Nothing chosen"}
            </span>
            {normalized === LOCAL_CRS ? (
              <span
                style={{
                  fontSize: "var(--text-md)",
                  color: "var(--text-secondary)",
                }}
              >
                Not placed on the earth
              </span>
            ) : landing ? (
              <span
                style={{
                  fontSize: "var(--text-md)",
                  color: landing.plausible
                    ? "var(--text-primary)"
                    : "var(--status-error)",
                  fontFamily: "var(--font-mono)",
                }}
              >
                {landing.plausible
                  ? `Puts this network at ${formatLatLon(landing.lat, landing.lon)}`
                  : "This system puts the network nowhere on earth"}
              </span>
            ) : normalized ? (
              <span
                style={{
                  fontSize: "var(--text-md)",
                  color: "var(--text-tertiary)",
                }}
              >
                {reading
                  ? "No definition available to check this"
                  : "No coordinates to check against"}
              </span>
            ) : null}
            {/* Picking the neighbouring UTM zone is the commonest mistake
                there is, and it is self-diagnosing once you can see where
                the network landed. */}
            {zoneHint && (
              <button
                type="button"
                onClick={() => setDraft(zoneHint.epsg)}
                style={{
                  alignSelf: "flex-start",
                  background: "transparent",
                  border: "none",
                  padding: 0,
                  color: "var(--accent)",
                  fontSize: "var(--text-sm)",
                  cursor: "pointer",
                  fontFamily: "var(--font-ui)",
                }}
              >
                That longitude is in UTM zone {zoneHint.zone} — use{" "}
                {zoneHint.epsg} instead
              </button>
            )}
          </div>
          <button
            type="button"
            className="tool-btn"
            onClick={dismiss}
            style={{ width: "auto", height: 28, padding: "0 10px" }}
          >
            Cancel
          </button>
          <button
            type="button"
            className="tool-btn"
            onClick={() => void save()}
            disabled={saving || !dirty || !normalized}
            style={{ width: "auto", height: 28, padding: "0 10px" }}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </ModalBackdrop>
  );
}
