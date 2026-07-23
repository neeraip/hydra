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
 * For node deletions the dialog warns that connected links will also be removed.
 */

import { ExclamationTriangleIcon } from "@heroicons/react/16/solid";
import { type ReactNode, useEffect, useRef } from "react";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

const NODE_KINDS = new Set(["junction", "reservoir", "tank"]);

interface DeleteConfirmModalProps {
  open: boolean;
  elementKind: string;
  elementId: string;
  /** Overrides the default "Delete {Kind}" heading. */
  title?: string;
  /** Overrides the default "Delete {id}?" body text. */
  message?: ReactNode;
  /** Overrides the destructive button label (default "Delete"). */
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function DeleteConfirmModal({
  open,
  elementKind,
  elementId,
  title,
  message,
  confirmLabel = "Delete",
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

  const isNode = NODE_KINDS.has(elementKind);
  const kindLabel = elementKind.charAt(0).toUpperCase() + elementKind.slice(1);

  return (
    <ModalBackdrop
      onDismiss={onCancel}
      zIndex={200}
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
                fontSize: 14,
                fontWeight: 600,
                color: "var(--text-primary)",
              }}
            >
              {title ?? `Delete ${kindLabel}`}
            </p>
            <p
              style={{
                margin: "4px 0 0",
                fontSize: 12,
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
                  ?{isNode && <> All connected links will also be removed.</>}
                </>
              )}
            </p>
          </div>
        </div>

        {/* Actions */}
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
          }}
        >
          <button
            type="button"
            ref={cancelRef}
            onClick={onCancel}
            style={{
              background: "transparent",
              border: "1px solid var(--border)",
              borderRadius: 6,
              padding: "6px 14px",
              fontSize: 12,
              fontWeight: 500,
              color: "var(--text-secondary)",
              cursor: "pointer",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-secondary)";
            }}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            style={{
              background: "#ef4444",
              border: "none",
              borderRadius: 6,
              padding: "6px 14px",
              fontSize: 12,
              fontWeight: 600,
              color: "#fff",
              cursor: "pointer",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "#dc2626";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "#ef4444";
            }}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </ModalBackdrop>
  );
}
