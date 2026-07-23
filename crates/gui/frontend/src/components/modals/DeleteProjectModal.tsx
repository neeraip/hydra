/**
 * DeleteProjectModal — confirmation dialog for permanently deleting a project
 * bundle from disk (all its files, not just removing it from the list).
 *
 * Usage:
 *   <DeleteProjectModal
 *     open={!!pendingDeleteProject}
 *     projectName={pendingDeleteProject?.name ?? ""}
 *     onConfirm={handleDelete}
 *     onCancel={() => setPendingDeleteProject(null)}
 *   />
 */

import { ExclamationTriangleIcon } from "@heroicons/react/16/solid";
import { useEffect, useRef } from "react";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

interface DeleteProjectModalProps {
  open: boolean;
  projectName: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function DeleteProjectModal({
  open,
  projectName,
  onConfirm,
  onCancel,
}: DeleteProjectModalProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  // Focus the Cancel button when the modal opens — a stray Enter must never
  // instantly confirm a permanent project deletion.
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

  return (
    <ModalBackdrop
      onDismiss={onCancel}
      zIndex={200}
      background="rgba(0,0,0,0.55)"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="delete-project-modal-title"
        {...stopBackdropEvents}
        style={{
          background: "var(--bg-panel)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: "24px 28px",
          width: 400,
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
              id="delete-project-modal-title"
              style={{
                margin: 0,
                fontSize: 14,
                fontWeight: 600,
                color: "var(--text-primary)",
              }}
            >
              Delete project
            </p>
            <p
              style={{
                margin: "4px 0 0",
                fontSize: 12,
                color: "var(--text-secondary)",
                lineHeight: 1.5,
              }}
            >
              Permanently delete{" "}
              <strong style={{ color: "var(--text-primary)" }}>
                {projectName}
              </strong>{" "}
              and all its files? This cannot be undone.
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
            Delete
          </button>
        </div>
      </div>
    </ModalBackdrop>
  );
}
