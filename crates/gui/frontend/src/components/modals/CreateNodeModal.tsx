import { useEffect, useRef, useState } from "react";

export interface NodeCreatePayload {
  kind: string;
  id: string;
  /** Elevation / head in metres. For tanks this is the bottom elevation. */
  elevation: number;
  minLevel: number;
  maxLevel: number;
  initialLevel: number;
}

interface Props {
  open: boolean;
  /** Returns a suggested ID for the given node kind prefix. */
  suggestId: (kind: string) => string;
  /** Click location in geographic coordinates. */
  lng: number;
  lat: number;
  onConfirm: (payload: NodeCreatePayload) => Promise<void>;
  onCancel: () => void;
}

const NODE_TYPES = [
  { value: "junction", label: "Junction" },
  { value: "reservoir", label: "Reservoir" },
  { value: "tank", label: "Tank" },
];

export function CreateNodeModal({
  open,
  suggestId,
  lng,
  lat,
  onConfirm,
  onCancel,
}: Props) {
  const [kind, setKind] = useState("junction");
  const [id, setId] = useState(() => suggestId("junction"));
  const [elevation, setElevation] = useState("0");
  const [minLevel, setMinLevel] = useState("0");
  const [maxLevel, setMaxLevel] = useState("3");
  const [initialLevel, setInitialLevel] = useState("1.5");
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const idRef = useRef<HTMLInputElement>(null);
  // True once the user has manually typed something — stops auto-update on type switch.
  const userEditedRef = useRef(false);

  // Reset fields and focus ID when modal opens.
  useEffect(() => {
    if (!open) return;
    userEditedRef.current = false;
    setKind("junction");
    setId(suggestId("junction"));
    setElevation("0");
    setMinLevel("0");
    setMaxLevel("3");
    setInitialLevel("1.5");
    setErrorMsg(null);
    requestAnimationFrame(() => {
      idRef.current?.select();
      idRef.current?.focus();
    });
    // suggestId is stable (useCallback in parent), safe to omit from deps.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, suggestId]);

  // Update the suggested ID when the user switches type — unless they've customised it.
  function handleKindChange(newKind: string) {
    setKind(newKind);
    setErrorMsg(null);
    if (!userEditedRef.current) {
      setId(suggestId(newKind));
    }
  }

  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        onCancel();
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, onCancel]);

  if (!open) return null;

  const trimmedId = id.trim();
  const canSubmit = !!trimmedId && !submitting;

  const elevLabel = kind === "reservoir" ? "Head (m)" : "Elevation (m)";

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitting(true);
    setErrorMsg(null);
    try {
      await onConfirm({
        kind,
        id: trimmedId,
        elevation: parseFloat(elevation) || 0,
        minLevel: parseFloat(minLevel) || 0,
        maxLevel: parseFloat(maxLevel) || 3,
        initialLevel: parseFloat(initialLevel) || 1.5,
      });
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

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

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the modal on pointer interaction.
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 2000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.55)",
      }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        style={{
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: "20px 24px",
          width: 340,
          boxShadow: "0 8px 32px rgba(0,0,0,0.45)",
          display: "flex",
          flexDirection: "column",
          gap: 14,
        }}
      >
        <span
          style={{
            fontWeight: 600,
            fontSize: 14,
            color: "var(--text-primary)",
          }}
        >
          Add node
        </span>

        {/* Node type */}
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={labelStyle}>Type</span>
          <div style={{ display: "flex", gap: 6 }}>
            {NODE_TYPES.map((t) => (
              <button
                type="button"
                key={t.value}
                onClick={() => handleKindChange(t.value)}
                style={{
                  flex: 1,
                  padding: "5px 0",
                  borderRadius: 6,
                  fontSize: 12,
                  fontWeight: 500,
                  border:
                    kind === t.value
                      ? "1px solid var(--accent)"
                      : "1px solid var(--border)",
                  background:
                    kind === t.value ? "var(--accent-dim)" : "var(--bg-input)",
                  color:
                    kind === t.value
                      ? "var(--accent)"
                      : "var(--text-secondary)",
                  cursor: "pointer",
                }}
              >
                {t.label}
              </button>
            ))}
          </div>
        </div>

        {/* ID */}
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={labelStyle}>ID</span>
          <input
            ref={idRef}
            value={id}
            onChange={(e) => {
              userEditedRef.current = true;
              setId(e.target.value);
              setErrorMsg(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSubmit();
            }}
            style={{
              ...fieldStyle,
              borderColor: errorMsg ? "rgba(220,60,60,0.6)" : "var(--border)",
            }}
            placeholder="e.g. J1"
          />
          {errorMsg && (
            <span
              style={{
                fontSize: 11,
                color: "rgba(220,60,60,0.9)",
                marginTop: 2,
              }}
            >
              {errorMsg}
            </span>
          )}
        </label>

        {/* Elevation / Head */}
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={labelStyle}>{elevLabel}</span>
          <input
            type="number"
            value={elevation}
            onChange={(e) => setElevation(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSubmit();
            }}
            style={fieldStyle}
          />
        </label>

        {/* Tank level fields */}
        {kind === "tank" && (
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "1fr 1fr 1fr",
              gap: 8,
            }}
          >
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={labelStyle}>Min lvl (m)</span>
              <input
                type="number"
                value={minLevel}
                onChange={(e) => setMinLevel(e.target.value)}
                style={fieldStyle}
              />
            </label>
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={labelStyle}>Max lvl (m)</span>
              <input
                type="number"
                value={maxLevel}
                onChange={(e) => setMaxLevel(e.target.value)}
                style={fieldStyle}
              />
            </label>
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={labelStyle}>Init lvl (m)</span>
              <input
                type="number"
                value={initialLevel}
                onChange={(e) => setInitialLevel(e.target.value)}
                style={fieldStyle}
              />
            </label>
          </div>
        )}

        {/* Coordinates (read-only info) */}
        <div
          style={{
            fontSize: 11,
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {lng.toFixed(6)}, {lat.toFixed(6)}
        </div>

        {/* Actions */}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button
            type="button"
            className="tool-btn"
            onClick={onCancel}
            disabled={submitting}
            style={{ fontSize: 12 }}
          >
            Cancel
          </button>
          <button
            type="button"
            className="tool-btn"
            disabled={!canSubmit}
            onClick={handleSubmit}
            style={{
              fontSize: 12,
              background: canSubmit ? "var(--accent)" : undefined,
              color: canSubmit ? "#fff" : undefined,
              opacity: canSubmit ? 1 : 0.5,
            }}
          >
            {submitting ? "Adding…" : "Add"}
          </button>
        </div>
      </div>
    </div>
  );
}
