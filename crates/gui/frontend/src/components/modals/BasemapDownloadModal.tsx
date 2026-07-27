/**
 * BasemapDownloadModal — plan and start an offline-basemap region download.
 *
 * Opened from Settings (manual bbox entry) or from the canvas coverage chip
 * (bbox prefilled from the viewport). On open with a valid bbox it calls
 * `plan_basemap_download` (slow — network) and shows the exact byte cost;
 * Confirm starts the background download. Progress renders from
 * BasemapDownloadContext, which owns the event subscription — closing the
 * modal never interrupts the download (completion arrives as a toast).
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useBasemapDownload } from "../../hooks/BasemapDownloadContext";
import {
  type BasemapBbox,
  type BasemapDownloadPlan,
  bboxFromStrings,
  formatBytes,
  planBasemapDownload,
} from "../../hooks/basemaps";
import { formatIpcError } from "../../hooks/ipc";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

interface BasemapDownloadModalProps {
  open: boolean;
  /** Prefilled bbox (e.g. padded viewport), or null for manual entry. */
  initialBbox: BasemapBbox | null;
  /** Default region name (project name when opened in a project context). */
  initialName: string;
  /** When set, the downloaded region is linked to this project. */
  projectId?: string | null;
  onClose: () => void;
}

type PlanState =
  | { status: "idle" }
  | { status: "planning" }
  | { status: "ready"; plan: BasemapDownloadPlan; bboxKey: string }
  | { status: "error"; message: string };

const fieldStyle: React.CSSProperties = {
  background: "var(--bg-input)",
  border: "1px solid var(--border)",
  borderRadius: 6,
  padding: "6px 10px",
  fontSize: 13,
  color: "var(--text-primary)",
  outline: "none",
  width: "100%",
  boxSizing: "border-box",
};

const labelStyle: React.CSSProperties = {
  fontSize: 11,
  color: "var(--text-tertiary)",
  textTransform: "uppercase",
  letterSpacing: "0.06em",
};

/** Small inline spinner (border-based, no assets). */
function Spinner() {
  return (
    <span
      aria-hidden
      style={{
        width: 12,
        height: 12,
        flexShrink: 0,
        border: "2px solid var(--border-hover)",
        borderTopColor: "var(--accent)",
        borderRadius: "50%",
        display: "inline-block",
        animation: "spin 800ms linear infinite",
      }}
    />
  );
}

export function BasemapDownloadModal({
  open,
  initialBbox,
  initialName,
  projectId,
  onClose,
}: BasemapDownloadModalProps) {
  const { active, startDownload, cancelDownload } = useBasemapDownload();
  const [name, setName] = useState(initialName);
  const [minLon, setMinLon] = useState("");
  const [minLat, setMinLat] = useState("");
  const [maxLon, setMaxLon] = useState("");
  const [maxLat, setMaxLat] = useState("");
  const [planState, setPlanState] = useState<PlanState>({ status: "idle" });
  const [startError, setStartError] = useState<string | null>(null);
  // True once THIS modal instance started a download — drives auto-close
  // when the download leaves the active state (toast reports the outcome).
  const [started, setStarted] = useState(false);
  const planSeqRef = useRef(0);

  const bbox = bboxFromStrings(minLon, minLat, maxLon, maxLat);
  const bboxKey = bbox ? bbox.map((v) => v.toFixed(6)).join(",") : null;

  const runPlan = useCallback(async (b: BasemapBbox) => {
    const seq = ++planSeqRef.current;
    setPlanState({ status: "planning" });
    try {
      const plan = await planBasemapDownload(b);
      if (planSeqRef.current !== seq) return; // superseded by a later plan
      setPlanState({
        status: "ready",
        plan,
        bboxKey: b.map((v) => v.toFixed(6)).join(","),
      });
    } catch (err) {
      if (planSeqRef.current !== seq) return;
      setPlanState({ status: "error", message: formatIpcError(err) });
    }
  }, []);

  // Reset fields on open; auto-plan when a bbox was prefilled by the caller.
  // biome-ignore lint/correctness/useExhaustiveDependencies: initial* props are only read at open time by design.
  useEffect(() => {
    if (!open) return;
    setName(initialName);
    setStartError(null);
    setStarted(false);
    if (initialBbox) {
      setMinLon(String(initialBbox[0].toFixed(5)));
      setMinLat(String(initialBbox[1].toFixed(5)));
      setMaxLon(String(initialBbox[2].toFixed(5)));
      setMaxLat(String(initialBbox[3].toFixed(5)));
      void runPlan(initialBbox);
    } else {
      setMinLon("");
      setMinLat("");
      setMaxLon("");
      setMaxLat("");
      setPlanState({ status: "idle" });
    }
  }, [open]);

  // Close on Escape (the download, if started, continues headless).
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, onClose]);

  // Auto-close once the download this modal started has finished (the
  // context already toasted the outcome).
  useEffect(() => {
    if (open && started && active === null) onClose();
  }, [open, started, active, onClose]);

  if (!open) return null;

  const planIsCurrent =
    planState.status === "ready" && bboxKey === planState.bboxKey;
  const trimmedName = name.trim();
  const downloading = started && active !== null;
  const canConfirm =
    !downloading && planIsCurrent && trimmedName.length > 0 && bbox !== null;

  const handleBboxField = (setter: (v: string) => void) => (v: string) => {
    setter(v);
    // Any edit invalidates a computed plan — require a fresh estimate.
    setPlanState((p) => (p.status === "planning" ? p : { status: "idle" }));
  };

  async function handleConfirm() {
    if (!canConfirm || !bbox) return;
    setStartError(null);
    try {
      setStarted(true);
      await startDownload({
        name: trimmedName,
        bbox,
        projectId: projectId ?? null,
      });
    } catch (err) {
      setStarted(false);
      setStartError(formatIpcError(err));
    }
  }

  const progressPct =
    active && active.totalBytes > 0
      ? Math.min(100, (active.doneBytes / active.totalBytes) * 100)
      : 0;

  return (
    <ModalBackdrop
      onDismiss={onClose}
      zIndex={2000}
      background="rgba(0,0,0,0.55)"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="basemap-download-modal-title"
        {...stopBackdropEvents}
        style={{
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: "20px 24px",
          width: 380,
          boxShadow: "0 8px 32px rgba(0,0,0,0.45)",
          display: "flex",
          flexDirection: "column",
          gap: 14,
        }}
      >
        <span
          id="basemap-download-modal-title"
          style={{
            fontWeight: 600,
            fontSize: 14,
            color: "var(--text-primary)",
          }}
        >
          Download offline basemap
        </span>

        {/* Region name */}
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={labelStyle}>Region name</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            disabled={downloading}
            style={fieldStyle}
            placeholder="e.g. Springfield"
          />
        </label>

        {/* Bounding box */}
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={labelStyle}>Bounding box (WGS84°)</span>
          <div
            style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}
          >
            {(
              [
                ["Min lon", minLon, handleBboxField(setMinLon)],
                ["Min lat", minLat, handleBboxField(setMinLat)],
                ["Max lon", maxLon, handleBboxField(setMaxLon)],
                ["Max lat", maxLat, handleBboxField(setMaxLat)],
              ] as const
            ).map(([label, value, set]) => (
              <label
                key={label}
                style={{ display: "flex", flexDirection: "column", gap: 2 }}
              >
                <span style={{ fontSize: 10, color: "var(--text-tertiary)" }}>
                  {label}
                </span>
                <input
                  type="number"
                  value={value}
                  onChange={(e) => set(e.target.value)}
                  disabled={downloading}
                  style={fieldStyle}
                />
              </label>
            ))}
          </div>
          {bbox === null &&
            (minLon || minLat || maxLon || maxLat) &&
            !downloading && (
              <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
                Enter a valid box: min &lt; max, lon ±180, lat ±90.
              </span>
            )}
        </div>

        {/* Plan / progress area */}
        {downloading && active ? (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                fontSize: 12,
                color: "var(--text-secondary)",
              }}
            >
              <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Spinner />
                {active.phase === "planning"
                  ? "Planning download…"
                  : `Downloading ${active.regionName}…`}
              </span>
              <span
                style={{
                  fontVariantNumeric: "tabular-nums",
                  color: "var(--accent)",
                  fontWeight: 600,
                }}
              >
                {active.totalBytes > 0
                  ? `${formatBytes(active.doneBytes)} / ${formatBytes(active.totalBytes)}`
                  : ""}
              </span>
            </div>
            <div
              style={{
                height: 4,
                background: "var(--border)",
                borderRadius: 2,
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  width: `${progressPct}%`,
                  height: "100%",
                  background: "var(--accent)",
                  borderRadius: 2,
                  transition: "width 400ms ease",
                }}
              />
            </div>
            <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
              Closing this window keeps the download running.
            </span>
          </div>
        ) : planState.status === "planning" ? (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 12,
              color: "var(--text-secondary)",
            }}
          >
            <Spinner />
            Estimating download size…
          </div>
        ) : planIsCurrent && planState.status === "ready" ? (
          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
            <span style={{ fontSize: 13, color: "var(--text-primary)" }}>
              <strong>{formatBytes(planState.plan.newBytes)}</strong> to
              download ({formatBytes(planState.plan.sharedBytes)} already on
              disk)
            </span>
            <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
              {planState.plan.missingTiles.toLocaleString()} tiles to fetch ·{" "}
              {planState.plan.presentTiles.toLocaleString()} already stored
            </span>
          </div>
        ) : planState.status === "error" ? (
          <span style={{ fontSize: 12, color: "var(--status-error)" }}>
            Size estimate failed: {planState.message}
          </span>
        ) : (
          <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            Estimate the download size to continue.
          </span>
        )}

        {startError && (
          <span style={{ fontSize: 12, color: "var(--status-error)" }}>
            {startError}
          </span>
        )}

        {/* Actions */}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          {downloading ? (
            <>
              <button
                type="button"
                className="tool-btn"
                onClick={cancelDownload}
                style={{ fontSize: 12, color: "var(--status-error)" }}
              >
                Cancel download
              </button>
              <button
                type="button"
                className="tool-btn"
                onClick={onClose}
                style={{ fontSize: 12 }}
              >
                Close
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                className="tool-btn"
                onClick={onClose}
                style={{ fontSize: 12 }}
              >
                Cancel
              </button>
              {!planIsCurrent && planState.status !== "planning" && (
                <button
                  type="button"
                  className="tool-btn"
                  disabled={bbox === null}
                  onClick={() => bbox && runPlan(bbox)}
                  style={{ fontSize: 12, opacity: bbox === null ? 0.5 : 1 }}
                >
                  Estimate size
                </button>
              )}
              <button
                type="button"
                className="tool-btn"
                disabled={!canConfirm}
                onClick={handleConfirm}
                style={{
                  fontSize: 12,
                  background: canConfirm ? "var(--accent)" : undefined,
                  color: canConfirm ? "#fff" : undefined,
                  opacity: canConfirm ? 1 : 0.5,
                }}
              >
                Download
              </button>
            </>
          )}
        </div>
      </div>
    </ModalBackdrop>
  );
}
