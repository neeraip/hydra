import { useEffect } from "react";
import { useAppState } from "../../AppContext";

const AUTO_DISMISS_MS: Record<string, number> = {
  success: 2500,
  info: 2500,
  warn: 3500,
  error: 5000,
};

export function Toast() {
  const { toast, dismissToast } = useAppState();

  useEffect(() => {
    if (!toast) return;
    const id = setTimeout(dismissToast, AUTO_DISMISS_MS[toast.type] ?? 2500);
    return () => clearTimeout(id);
  }, [toast, dismissToast]);

  if (!toast) return null;

  return (
    <div className="toast-container">
      <div
        className={`toast ${toast.type}`}
        role={toast.type === "error" ? "alert" : "status"}
      >
        {toast.message}
      </div>
    </div>
  );
}
