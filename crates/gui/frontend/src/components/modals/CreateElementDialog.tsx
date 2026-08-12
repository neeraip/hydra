// ── The "add an element" dialog ───────────────────────────────────────────────
//
// The chrome every create modal has: a backdrop that dismisses, a card, a
// heading, a row of kind buttons, an ID field that validates its own
// format, the backend's refusal shown under it, and Cancel/Add.
//
// One component rather than one per engine, because none of that is
// engine knowledge — the kinds come in as data, and whatever else a kind
// needs comes in as children. What differs between a water-distribution
// tank and a drainage junction is which numbers they carry, and that is
// exactly what the caller supplies.
//
// The ID rules are the dialog's own, though: an id with a space or a
// semicolon in it breaks INP tokenisation on the next save whichever
// engine wrote it, so the check belongs where every create passes
// through. Collisions are *not* checked here — only the model knows what
// is taken, and asking it is the backend's answer to return.

import { type ReactNode, useEffect, useRef, useState } from "react";
import type { ElementAttributeQuantity } from "../../hooks";
import { inpIdError } from "../../inpId";
import { EditableNumber } from "../ui/EditableNumber";

/** One selectable element kind. */
export interface CreateKind {
  value: string;
  label: string;
}

const LABEL: React.CSSProperties = {
  fontSize: "var(--text-sm)",
  color: "var(--text-tertiary)",
  textTransform: "uppercase",
  letterSpacing: "0.06em",
};

const FIELD: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
};

export function CreateElementDialog({
  open,
  title,
  kinds,
  kind,
  onKindChange,
  id,
  onIdChange,
  idPlaceholder,
  note,
  children,
  onSubmit,
  onCancel,
}: {
  open: boolean;
  /** "Add node", "Add link" — what the user is doing, not what it is. */
  title: string;
  kinds: CreateKind[];
  kind: string;
  onKindChange: (kind: string) => void;
  id: string;
  onIdChange: (id: string) => void;
  idPlaceholder?: string;
  /** A line under the fields: what the new element will default to. */
  note?: ReactNode;
  /** Whatever else this kind needs — between the ID and the note. */
  children?: ReactNode;
  /**
   * Create it. Rejecting keeps the dialog open with the message shown,
   * which is how a refusal reaches the person who can act on it: a
   * kind that needs a curve, an id already in use.
   */
  onSubmit: () => Promise<void>;
  onCancel: () => void;
}) {
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const idRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setErrorMsg(null);
    requestAnimationFrame(() => {
      idRef.current?.select();
      idRef.current?.focus();
    });
  }, [open]);

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

  // The format check runs as you type; the collision check is the
  // backend's, and arrives as a rejection. An empty field is not yet an
  // error — it is a field nobody has typed in.
  const idError = inpIdError(id);
  const shownIdError = id.trim() !== "" ? idError : null;
  const canSubmit = idError === null && !submitting;

  async function submit() {
    if (!canSubmit) return;
    setSubmitting(true);
    setErrorMsg(null);
    try {
      await onSubmit();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

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
        role="dialog"
        aria-label={title}
        style={{
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
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
            fontSize: "var(--text-xl)",
            color: "var(--text-primary)",
          }}
        >
          {title}
        </span>

        {/* A single kind is not a choice, and a row of one button reads
            like the others are loading. Say what it is instead. */}
        <div style={FIELD}>
          <span style={LABEL}>Type</span>
          {kinds.length === 1 ? (
            <span
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-secondary)",
              }}
            >
              {kinds[0].label}
            </span>
          ) : (
            <div style={{ display: "flex", gap: 6 }}>
              {kinds.map((t) => (
                <button
                  type="button"
                  key={t.value}
                  onClick={() => {
                    setErrorMsg(null);
                    onKindChange(t.value);
                  }}
                  aria-pressed={kind === t.value}
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
                      kind === t.value
                        ? "var(--accent-dim)"
                        : "var(--bg-input)",
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
          )}
        </div>

        <label style={FIELD}>
          <span style={LABEL}>ID</span>
          <input
            ref={idRef}
            value={id}
            aria-label="ID"
            onChange={(e) => {
              setErrorMsg(null);
              onIdChange(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
            style={{
              background: "var(--bg-input)",
              border: `1px solid ${
                errorMsg || shownIdError
                  ? "rgba(220,60,60,0.6)"
                  : "var(--border)"
              }`,
              borderRadius: 6,
              padding: "6px 10px",
              fontSize: "var(--text-lg)",
              color: "var(--text-primary)",
              outline: "none",
            }}
            placeholder={idPlaceholder}
          />
          {(errorMsg ?? shownIdError) && (
            <span
              role="alert"
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

        {children}

        {note && (
          <div
            style={{
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
            }}
          >
            {note}
          </div>
        )}

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
            onClick={() => void submit()}
            style={{
              fontSize: "var(--text-md)",
              background: "var(--accent-dim)",
              color: "var(--accent)",
              borderColor: "var(--accent)",
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

/**
 * One numeric field in a create dialog: a label, the app's editable
 * number, and the unit beside it.
 *
 * A `div` rather than a `label`, because the input is inside a component
 * and cannot be associated with an outer `label` element. The name
 * reaches assistive technology through the input's own `aria-label`,
 * which `EditableNumber` sets from the same string — a `label` that
 * associates with nothing announces less than that, while looking like
 * it announces more.
 */
export function CreateNumberField({
  label,
  value,
  quantity,
  sys,
  onCommit,
}: {
  label: string;
  /** In the unit the backend takes — SI for a quantity-bearing value. */
  value: number;
  quantity?: ElementAttributeQuantity;
  sys: "si" | "us";
  onCommit: (value: number) => void;
}) {
  const unit = quantity
    ? sys === "us"
      ? quantity.usLabel
      : quantity.siLabel
    : "";
  return (
    <div style={FIELD}>
      <span style={LABEL}>{label}</span>
      <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <EditableNumber
          value={value}
          quantity={quantity}
          sys={sys}
          label={label}
          width={100}
          align="left"
          onCommit={onCommit}
        />
        {unit && <span style={{ color: "var(--text-tertiary)" }}>{unit}</span>}
      </span>
    </div>
  );
}
