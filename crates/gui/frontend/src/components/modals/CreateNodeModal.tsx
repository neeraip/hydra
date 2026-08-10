import { useEffect, useRef, useState } from "react";
import type { CanvasPoint } from "../../canvas/types";
import { inpIdError } from "../../inpId";
import { fromDisplay, toDisplay, unitLabel, useUnitSystem } from "../../units";

/**
 * The click position, read back to confirm where the node will land.
 *
 * Degrees get six decimals, which is about a tenth of a metre; a plan's
 * own coordinates get three, because they are already in the model's
 * units and six would claim a micron. The label says which is which —
 * "4.89, 52.37" and "4890, 52370" are otherwise the same two numbers.
 */
export function formatDropPoint(at: CanvasPoint): string {
  return at.space === "wgs84"
    ? `${at.x.toFixed(6)}, ${at.y.toFixed(6)} (lon, lat)`
    : `${at.x.toFixed(3)}, ${at.y.toFixed(3)} (model grid)`;
}

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
  /** Where the click landed, and in which space. Null while closed. */
  at: CanvasPoint | null;
  onConfirm: (payload: NodeCreatePayload) => Promise<void>;
  onCancel: () => void;
}

const NODE_TYPES = [
  { value: "junction", label: "Junction" },
  { value: "reservoir", label: "Reservoir" },
  { value: "tank", label: "Tank" },
];

/** SI defaults for the tank level fields (metres). */
const DEFAULT_MIN_LEVEL_M = 0;
const DEFAULT_MAX_LEVEL_M = 3;
const DEFAULT_INITIAL_LEVEL_M = 1.5;

export function CreateNodeModal({
  open,
  suggestId,
  at,
  onConfirm,
  onCancel,
}: Props) {
  const sys = useUnitSystem();
  // Level defaults are stored SI and presented in the display system.
  const dispStr = (siValue: number) =>
    String(Number(toDisplay(siValue, "length", sys).toFixed(2)));
  const [kind, setKind] = useState("junction");
  const [id, setId] = useState(() => suggestId("junction"));
  const [elevation, setElevation] = useState("0");
  const [minLevel, setMinLevel] = useState(() => dispStr(DEFAULT_MIN_LEVEL_M));
  const [maxLevel, setMaxLevel] = useState(() => dispStr(DEFAULT_MAX_LEVEL_M));
  const [initialLevel, setInitialLevel] = useState(() =>
    dispStr(DEFAULT_INITIAL_LEVEL_M),
  );
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const idRef = useRef<HTMLInputElement>(null);
  // True once the user has manually typed something — stops auto-update on type switch.
  const userEditedRef = useRef(false);

  // Reset fields and focus ID when modal opens.
  // biome-ignore lint/correctness/useExhaustiveDependencies: dispStr derives only from `sys`; resetting on unit-system change while open is intended.
  useEffect(() => {
    if (!open) return;
    userEditedRef.current = false;
    setKind("junction");
    setId(suggestId("junction"));
    setElevation("0");
    setMinLevel(dispStr(DEFAULT_MIN_LEVEL_M));
    setMaxLevel(dispStr(DEFAULT_MAX_LEVEL_M));
    setInitialLevel(dispStr(DEFAULT_INITIAL_LEVEL_M));
    setErrorMsg(null);
    requestAnimationFrame(() => {
      idRef.current?.select();
      idRef.current?.focus();
    });
    // suggestId is stable (useCallback in parent), safe to omit from deps.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, suggestId, sys]);

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
  // Format is checked inline so the message lands next to the field; the
  // backend runs the same rules and owns collisions. Held back until the user
  // has typed something, so an untouched empty field is not scolded.
  const idError = inpIdError(id);
  const shownIdError = trimmedId !== "" ? idError : null;
  const canSubmit = idError === null && !submitting;

  const elevLabel =
    kind === "reservoir"
      ? `Head (${unitLabel("head", sys)})`
      : `Elevation (${unitLabel("elevation", sys)})`;

  /** Parse a display-unit field back to SI, falling back to an SI default.
   * Mirrors the previous `parseFloat(x) || default` semantics (NaN/0 → default). */
  function parseSi(raw: string, fallbackSi: number): number {
    const n = parseFloat(raw);
    return n ? fromDisplay(n, "length", sys) : fallbackSi;
  }

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitting(true);
    setErrorMsg(null);
    try {
      await onConfirm({
        kind,
        id: trimmedId,
        elevation: parseSi(elevation, 0),
        minLevel: parseSi(minLevel, DEFAULT_MIN_LEVEL_M),
        maxLevel: parseSi(maxLevel, DEFAULT_MAX_LEVEL_M),
        initialLevel: parseSi(initialLevel, DEFAULT_INITIAL_LEVEL_M),
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
    fontSize: "var(--text-lg)",
    color: "var(--text-primary)",
    outline: "none",
    width: "100%",
    boxSizing: "border-box",
  };
  const labelStyle: React.CSSProperties = {
    fontSize: "var(--text-sm)",
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
            fontSize: "var(--text-xl)",
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
                  fontSize: "var(--text-md)",
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
              borderColor:
                errorMsg || shownIdError
                  ? "rgba(220,60,60,0.6)"
                  : "var(--border)",
            }}
            placeholder="e.g. J1"
          />
          {(errorMsg ?? shownIdError) && (
            <span
              style={{
                fontSize: "var(--text-sm)",
                color: "rgba(220,60,60,0.9)",
                marginTop: 2,
              }}
            >
              {errorMsg ?? shownIdError}
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
              <span style={labelStyle}>
                Min lvl ({unitLabel("length", sys)})
              </span>
              <input
                type="number"
                value={minLevel}
                onChange={(e) => setMinLevel(e.target.value)}
                style={fieldStyle}
              />
            </label>
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={labelStyle}>
                Max lvl ({unitLabel("length", sys)})
              </span>
              <input
                type="number"
                value={maxLevel}
                onChange={(e) => setMaxLevel(e.target.value)}
                style={fieldStyle}
              />
            </label>
            <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <span style={labelStyle}>
                Init lvl ({unitLabel("length", sys)})
              </span>
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
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {at ? formatDropPoint(at) : ""}
        </div>

        {/* Actions */}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button
            type="button"
            className="tool-btn"
            onClick={onCancel}
            disabled={submitting}
            style={{ fontSize: "var(--text-md)" }}
          >
            Cancel
          </button>
          <button
            type="button"
            className="tool-btn"
            disabled={!canSubmit}
            onClick={handleSubmit}
            style={{
              fontSize: "var(--text-md)",
              background: canSubmit ? "var(--accent)" : undefined,
              color: canSubmit ? "var(--accent-fg)" : undefined,
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
