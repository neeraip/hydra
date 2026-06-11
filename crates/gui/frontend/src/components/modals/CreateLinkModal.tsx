import { useEffect, useRef, useState } from "react";

interface Props {
  open: boolean;
  /** Returns a suggested ID for the given link kind. */
  suggestId: (kind: string) => string;
  fromNodeId: string;
  toNodeId: string;
  onConfirm: (kind: string, id: string) => Promise<void>;
  onCancel: () => void;
}

const LINK_TYPES = [
  { value: "pipe", label: "Pipe" },
  { value: "pump", label: "Pump" },
  { value: "valve", label: "Valve" },
];

export function CreateLinkModal({
  open,
  suggestId,
  fromNodeId,
  toNodeId,
  onConfirm,
  onCancel,
}: Props) {
  const [kind, setKind] = useState("pipe");
  const [id, setId] = useState(() => suggestId("pipe"));
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const idRef = useRef<HTMLInputElement>(null);
  const userEditedRef = useRef(false);

  useEffect(() => {
    if (!open) return;
    userEditedRef.current = false;
    setKind("pipe");
    setId(suggestId("pipe"));
    setErrorMsg(null);
    requestAnimationFrame(() => {
      idRef.current?.select();
      idRef.current?.focus();
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, suggestId]);

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

  const trimmed = id.trim();
  const canSubmit = !!trimmed && !submitting;

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitting(true);
    setErrorMsg(null);
    try {
      await onConfirm(kind, trimmed);
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
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
          border: "1px solid var(--border-subtle)",
          borderRadius: 10,
          padding: "20px 24px",
          width: 320,
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
          Add link
        </span>

        {/* Link type */}
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              textTransform: "uppercase",
              letterSpacing: "0.06em",
            }}
          >
            Type
          </span>
          <div style={{ display: "flex", gap: 6 }}>
            {LINK_TYPES.map((t) => (
              <button
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
                      : "1px solid var(--border-subtle)",
                  background:
                    kind === t.value
                      ? "var(--accent-dim)"
                      : "var(--bg-surface)",
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
        </label>

        {/* ID */}
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              textTransform: "uppercase",
              letterSpacing: "0.06em",
            }}
          >
            ID
          </span>
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
              background: "var(--bg-surface)",
              border: `1px solid ${errorMsg ? "rgba(220,60,60,0.6)" : "var(--border-subtle)"}`,
              borderRadius: 6,
              padding: "6px 10px",
              fontSize: 13,
              color: "var(--text-primary)",
              outline: "none",
            }}
            placeholder="e.g. P1"
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

        {/* From / To */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: 8,
            background: "var(--bg-surface)",
            borderRadius: 6,
            padding: "8px 10px",
          }}
        >
          <div>
            <div
              style={{
                fontSize: 10,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
                marginBottom: 2,
              }}
            >
              From
            </div>
            <div
              style={{
                fontSize: 12,
                color: "var(--text-primary)",
                fontFamily: "var(--font-mono)",
              }}
            >
              {fromNodeId}
            </div>
          </div>
          <div>
            <div
              style={{
                fontSize: 10,
                color: "var(--text-tertiary)",
                textTransform: "uppercase",
                letterSpacing: "0.06em",
                marginBottom: 2,
              }}
            >
              To
            </div>
            <div
              style={{
                fontSize: 12,
                color: "var(--text-primary)",
                fontFamily: "var(--font-mono)",
              }}
            >
              {toNodeId}
            </div>
          </div>
        </div>

        {/* Default params reminder */}
        <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
          {kind === "pipe"
            ? "Defaults: 100 m · 300 mm · C 100"
            : "Defaults: 10 kW constant-power"}
        </div>

        {/* Actions */}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button
            className="tool-btn"
            onClick={onCancel}
            disabled={submitting}
            style={{ fontSize: 12 }}
          >
            Cancel
          </button>
          <button
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
