/* Modal wrapper around the Scenarios management view. */

import { XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect } from "react";
import { useAppState } from "../../AppContext";
import { useScenarios } from "../../hooks";
import { ScenariosPanel } from "../panels/ScenariosPanel";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

export function ScenariosModal() {
  const {
    scenariosModalOpen,
    closeScenariosModal,
    activeProjectId,
    scenariosVersion,
  } = useAppState();
  const scenarios = useScenarios(activeProjectId, scenariosVersion);

  // Close on Escape.
  useEffect(() => {
    if (!scenariosModalOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeScenariosModal();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [scenariosModalOpen, closeScenariosModal]);

  if (!scenariosModalOpen) return null;

  return (
    <ModalBackdrop onDismiss={closeScenariosModal} zIndex={200}>
      <div
        {...stopBackdropEvents}
        style={{
          width: "min(860px, 90vw)",
          height: "min(640px, 85vh)",
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          backdropFilter: "blur(24px)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          boxShadow: "0 24px 80px rgba(0,0,0,0.5)",
        }}
      >
        {/* Modal title bar */}
        <div
          style={{
            flexShrink: 0,
            height: 52,
            borderBottom: "1px solid var(--border)",
            background: "var(--bg-panel)",
            display: "flex",
            alignItems: "center",
            padding: "0 16px",
            gap: 10,
          }}
        >
          <span
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Scenarios
          </span>
          <span
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            {scenarios.length} scenario{scenarios.length !== 1 ? "s" : ""}
          </span>
          <div style={{ flex: 1 }} />
          <button
            type="button"
            onClick={closeScenariosModal}
            aria-label="Close"
            style={{
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              width: 28,
              height: 28,
              border: "none",
              background: "transparent",
              color: "var(--text-secondary)",
              borderRadius: 5,
              cursor: "pointer",
              padding: 0,
              transition: "background var(--t-fast), color var(--t-fast)",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--nav-hover)";
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "transparent";
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-secondary)";
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        {/* ScenariosPanel fills the rest, header suppressed since modal provides it */}
        <ScenariosPanel showHeader={false} />
      </div>
    </ModalBackdrop>
  );
}
