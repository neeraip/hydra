import { useEffect, useState } from "react";
import { useAppState } from "../../AppContext";

/** Auto-dismiss delays per toast type. Errors linger longest so failure
 *  details can be read (and copied) before they disappear. */
const AUTO_DISMISS_MS: Record<string, number> = {
  success: 5000,
  info: 5000,
  warn: 5000,
  error: 15000,
};

/**
 * Toast stack — renders every visible toast (newest on top, capped by
 * AppContext). Each toast has its own dismiss timer, a close button, and
 * pauses its countdown while hovered.
 */
export function Toast() {
  const { toasts, dismissToast } = useAppState();

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container">
      {toasts.map((t) => (
        <ToastItem
          key={t.id}
          id={t.id}
          message={t.message}
          type={t.type}
          onDismiss={dismissToast}
        />
      ))}
    </div>
  );
}

function ToastItem({
  id,
  message,
  type,
  onDismiss,
}: {
  id: string;
  message: string;
  type: "info" | "success" | "warn" | "error";
  onDismiss: (id: string) => void;
}) {
  const [paused, setPaused] = useState(false);

  // Countdown restarts from the full duration when a hover pause ends —
  // simple and predictable, and errors are the only long-lived case anyway.
  useEffect(() => {
    if (paused) return;
    const handle = setTimeout(
      () => onDismiss(id),
      AUTO_DISMISS_MS[type] ?? 5000,
    );
    return () => clearTimeout(handle);
  }, [paused, id, type, onDismiss]);

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: hover only pauses the auto-dismiss countdown; the toast itself is a status/alert region, not an interactive control.
    <div
      className={`toast ${type}`}
      role={type === "error" ? "alert" : "status"}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
    >
      <span className="toast-message">{message}</span>
      <button
        type="button"
        className="toast-close"
        aria-label="Dismiss notification"
        onClick={() => onDismiss(id)}
      >
        ×
      </button>
    </div>
  );
}
