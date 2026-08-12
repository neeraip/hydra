import { useEffect, useRef, useState } from "react";
import { inpIdError } from "../../inpId";
import { DialogButton } from "../ui/DialogButton";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

// ─────────────────────────────────────────────────────────────────────────────
// Rename an element (node or link) from the Network Editor.
//
// Rename is an immediate, cascading operation (it rewrites every reference and
// clears undo history), so it is NOT staged into the editor draft — this small
// dialog fires it directly. Format validation mirrors the backend
// `validate_inp_id`; id collisions are reported by the backend as a toast.
// ─────────────────────────────────────────────────────────────────────────────

export function RenameElementModal({
  kind,
  id,
  onSubmit,
  onClose,
}: {
  kind: string;
  id: string;
  onSubmit: (newId: string) => void;
  onClose: () => void;
}) {
  const [value, setValue] = useState(id);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    inputRef.current?.select();
  }, []);

  const err = inpIdError(value);
  const unchanged = value.trim() === id;
  const canSubmit = err === null && !unchanged;

  const submit = () => {
    if (!canSubmit) return;
    onSubmit(value.trim());
  };

  // Esc closes; Enter submits. Window-level (matching the app's other modals)
  // so it works regardless of which control has focus.
  const submitRef = useRef(submit);
  submitRef.current = submit;
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "Enter") {
        e.preventDefault();
        submitRef.current();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <ModalBackdrop
      onDismiss={onClose}
      zIndex={210}
      style={{ animation: "fadeIn 120ms ease-out" }}
    >
      <div
        {...stopBackdropEvents}
        style={{
          width: "100%",
          maxWidth: 380,
          background: "var(--bg-panel)",
          backdropFilter: "blur(24px)",
          border: "1px solid var(--border-hover)",
          borderRadius: 12,
          boxShadow: "var(--shadow-3)",
          overflow: "hidden",
          animation: "scaleIn 160ms ease-out",
        }}
      >
        <div style={{ padding: "16px 20px" }}>
          <div
            style={{
              fontSize: "var(--text-xl)",
              fontWeight: 600,
              color: "var(--text-primary)",
              textTransform: "capitalize",
            }}
          >
            Rename {kind}
          </div>
          <div
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-tertiary)",
              marginTop: 2,
            }}
          >
            Currently{" "}
            <span style={{ fontFamily: "var(--font-mono)" }}>{id}</span>
          </div>

          <input
            ref={inputRef}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            style={{
              width: "100%",
              boxSizing: "border-box",
              marginTop: 12,
              fontSize: "var(--text-lg)",
              fontFamily: "var(--font-mono)",
              color: "var(--text-primary)",
              background: "var(--bg-card)",
              border: `1px solid ${err && value.trim() !== "" ? "var(--color-danger, #ef4444)" : "var(--border-hover)"}`,
              borderRadius: 6,
              padding: "8px 10px",
              outline: "none",
            }}
          />
          <div
            style={{
              fontSize: "var(--text-sm)",
              marginTop: 6,
              minHeight: 14,
              color:
                err && value.trim() !== ""
                  ? "var(--color-danger, #ef4444)"
                  : "var(--text-tertiary)",
            }}
          >
            {err && value.trim() !== ""
              ? err
              : "Updates every reference; clears undo history and marks results stale."}
          </div>
        </div>

        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 10,
            padding: "12px 20px",
            borderTop: "1px solid var(--border)",
            background: "rgba(0,0,0,0.18)",
          }}
        >
          <DialogButton onClick={onClose}>Cancel</DialogButton>
          <DialogButton intent="primary" disabled={!canSubmit} onClick={submit}>
            Rename
          </DialogButton>
        </div>
      </div>
    </ModalBackdrop>
  );
}
