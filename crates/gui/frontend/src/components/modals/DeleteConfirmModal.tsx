/**
 * DeleteConfirmModal — lightweight confirmation dialog for irreversible
 * element deletions.
 *
 * Usage:
 *   <DeleteConfirmModal
 *     open={!!pendingDelete}
 *     elementKind="junction"
 *     elementId="J-12"
 *     onConfirm={handleDelete}
 *     onCancel={() => setPendingDelete(null)}
 *   />
 *
 * The caller says whether the removal takes the links attached to the
 * element; the dialog warns when it does. It used to decide that from a
 * set of kind names — "junction", "reservoir", "tank" — which is one
 * engine's vocabulary living in a shared component: it warned for a
 * drainage junction by coincidence and stayed silent for an outfall,
 * storage unit and divider, all of which take their links too.
 */

import { ExclamationTriangleIcon } from "@heroicons/react/16/solid";
import { type ReactNode, useEffect, useRef } from "react";
import { DialogButton } from "../ui/DialogButton";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

/**
 * Confirmations sit above the surface that raised them.
 *
 * At the same z-index as an ordinary modal this only *looks* correct while
 * the confirmation happens to be a DOM descendant of its invoker — nesting
 * puts it inside that modal's stacking context. Raised from app level, as
 * the clear-results confirmation is, equal z-index means paint order decides
 * and the confirmation lands underneath the modal it belongs to. Above every
 * modal layer (200–300) is the only position that holds either way, and it
 * is the honest one: a confirmation is never the thing to be obscured.
 */
const CONFIRM_Z_INDEX = 400;

interface DeleteConfirmModalProps {
  open: boolean;
  elementKind: string;
  elementId: string;
  /** Whether removing this element also removes the links attached to
   * it, which the default message warns about. The caller decides: it
   * knows the element's class, where this dialog only has a kind name
   * it cannot interpret without knowing the engine. */
  takesLinks?: boolean;
  /** Overrides the default "Delete {Kind}" heading. */
  title?: string;
  /** Overrides the default "Delete {id}?" body text. */
  message?: ReactNode;
  /** Overrides the destructive button label (default "Delete"). */
  confirmLabel?: string;
  /**
   * An extra opt-in shown above the buttons — for a wider action the user may
   * want but must ask for. Always render it unchecked when the dialog opens:
   * a remembered checkbox on a destructive prompt is how someone deletes more
   * than they meant to.
   */
  option?: {
    label: string;
    checked: boolean;
    onChange: (checked: boolean) => void;
  };
  onConfirm: () => void;
  onCancel: () => void;
}

export function DeleteConfirmModal({
  open,
  elementKind,
  elementId,
  takesLinks = false,
  title,
  message,
  confirmLabel = "Delete",
  option,
  onConfirm,
  onCancel,
}: DeleteConfirmModalProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  // Focus the Cancel button when the modal opens — a stray Enter must never
  // instantly confirm a destructive action.
  useEffect(() => {
    if (open) cancelRef.current?.focus();
  }, [open]);

  // Close on Escape.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, onCancel]);

  if (!open) return null;

  const kindLabel = elementKind.charAt(0).toUpperCase() + elementKind.slice(1);

  return (
    <ModalBackdrop
      onDismiss={onCancel}
      zIndex={CONFIRM_Z_INDEX}
      background="rgba(0,0,0,0.55)"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="delete-modal-title"
        {...stopBackdropEvents}
        style={{
          background: "var(--bg-panel)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: "24px 28px",
          width: 380,
          display: "flex",
          flexDirection: "column",
          gap: 16,
          boxShadow: "0 24px 64px rgba(0,0,0,0.4)",
        }}
      >
        {/* Icon + title */}
        <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
          <div
            style={{
              flexShrink: 0,
              width: 36,
              height: 36,
              borderRadius: 8,
              background: "rgba(239,68,68,0.15)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <ExclamationTriangleIcon
              style={{ width: 18, height: 18, color: "#ef4444" }}
            />
          </div>
          <div>
            <p
              id="delete-modal-title"
              style={{
                margin: 0,
                fontSize: "var(--text-xl)",
                fontWeight: 600,
                color: "var(--text-primary)",
              }}
            >
              {title ?? `Delete ${kindLabel}`}
            </p>
            <p
              style={{
                margin: "4px 0 0",
                fontSize: "var(--text-md)",
                color: "var(--text-secondary)",
                lineHeight: 1.5,
              }}
            >
              {message ?? (
                <>
                  Delete{" "}
                  <strong style={{ color: "var(--text-primary)" }}>
                    {elementId}
                  </strong>
                  ?
                  {takesLinks && (
                    <> All connected links will also be removed.</>
                  )}
                </>
              )}
            </p>
          </div>
        </div>

        {option && (
          <label
            style={{
              display: "flex",
              alignItems: "flex-start",
              gap: 8,
              marginBottom: 16,
              fontSize: "var(--text-md)",
              lineHeight: 1.5,
              color: "var(--text-secondary)",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={option.checked}
              onChange={(e) => option.onChange(e.target.checked)}
              style={{
                accentColor: "var(--status-error, #e05c5c)",
                width: 13,
                height: 13,
                marginTop: 1,
                flexShrink: 0,
                cursor: "pointer",
              }}
            />
            {option.label}
          </label>
        )}

        {/* Actions */}
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
          }}
        >
          <DialogButton ref={cancelRef} onClick={onCancel}>
            Cancel
          </DialogButton>
          <DialogButton intent="danger" onClick={onConfirm}>
            {confirmLabel}
          </DialogButton>
        </div>
      </div>
    </ModalBackdrop>
  );
}
