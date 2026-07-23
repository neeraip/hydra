import { primaryModifierLabel, shiftModifierLabel } from "../../shortcuts";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

interface ShortcutRow {
  action: string;
  keys: string[];
}

interface ShortcutSection {
  title: string;
  rows: ShortcutRow[];
}

export function ShortcutCard({ onClose }: { onClose: () => void }) {
  const modifier = primaryModifierLabel();
  const shift = shiftModifierLabel();

  const sections: ShortcutSection[] = [
    {
      title: "Global",
      rows: [
        { action: "Command palette", keys: [modifier, "K"] },
        { action: "Run simulation", keys: [modifier, "R"] },
        { action: "Save editor changes", keys: [modifier, "S"] },
        { action: "Search projects", keys: [modifier, "F"] },
        { action: "Toggle geographic/orthogonal", keys: [modifier, "M"] },
        { action: "Zoom in", keys: [modifier, "="] },
        { action: "Zoom out", keys: [modifier, "-"] },
        { action: "Fit network", keys: [modifier, "0"] },
        { action: "Toggle issues panel", keys: [modifier, shift, "M"] },
        { action: "Keyboard shortcuts", keys: ["?"] },
        { action: "Go to Overview", keys: [modifier, "1"] },
        { action: "Go to Canvas", keys: [modifier, "2"] },
        { action: "Go to Editor", keys: [modifier, "3"] },
        { action: "Go to Analysis", keys: [modifier, "4"] },
      ],
    },
    {
      title: "Canvas",
      rows: [
        { action: "Use select tool", keys: ["S"] },
        { action: "Use edit tool", keys: ["E"] },
        { action: "Use add node tool", keys: ["N"] },
        { action: "Use add link tool", keys: ["L"] },
        { action: "Use measure tool", keys: ["D"] },
        { action: "Return to select tool", keys: ["Esc"] },
        { action: "Delete selected element", keys: ["Del", "Backspace"] },
        { action: "Select element", keys: ["Click"] },
        { action: "Zoom in/out", keys: ["Scroll"] },
      ],
    },
    {
      title: "Playback",
      rows: [
        { action: "Play / Pause", keys: ["Space"] },
        { action: "Step forward", keys: ["→"] },
        { action: "Step backward", keys: ["←"] },
        { action: "Jump to start", keys: ["Home"] },
        { action: "Jump to end", keys: ["End"] },
      ],
    },
  ];

  return (
    <ModalBackdrop
      onDismiss={onClose}
      zIndex={2000}
      style={{ animation: "fadeIn 120ms ease-out" }}
    >
      <div
        {...stopBackdropEvents}
        style={{
          background: "var(--bg-panel)",
          border: "1px solid var(--border)",
          borderRadius: 12,
          boxShadow: "0 24px 64px rgba(0,0,0,0.6)",
          maxWidth: 680,
          width: "100%",
          maxHeight: "80vh",
          overflowY: "auto",
          margin: "0 24px",
          animation: "scaleIn 180ms ease-out",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "20px 24px 16px",
            borderBottom: "1px solid var(--border)",
            position: "sticky",
            top: 0,
            background: "var(--bg-panel)",
            zIndex: 1,
          }}
        >
          <h2
            style={{
              margin: 0,
              fontSize: 17,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            Keyboard Shortcuts
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="modal-close-btn"
            style={{
              border: "none",
              background: "transparent",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              fontSize: 18,
              lineHeight: 1,
              padding: "4px 8px",
              borderRadius: 6,
              fontFamily: "var(--font-ui)",
              transition: "color var(--t-fast), background var(--t-fast)",
            }}
          >
            ×
          </button>
        </div>

        {/* Sections */}
        <div
          style={{
            padding: "8px 0 24px",
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: 0,
          }}
        >
          {sections.map((section) => (
            <div key={section.title} style={{ padding: "16px 24px" }}>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 700,
                  letterSpacing: "0.08em",
                  textTransform: "uppercase",
                  color: "var(--text-tertiary)",
                  marginBottom: 12,
                }}
              >
                {section.title}
              </div>
              <table style={{ width: "100%", borderCollapse: "collapse" }}>
                <tbody>
                  {section.rows.map((row) => (
                    <tr key={row.action}>
                      <td
                        style={{
                          padding: "5px 0",
                          fontSize: 13,
                          color: "var(--text-secondary)",
                          paddingRight: 16,
                        }}
                      >
                        {row.action}
                      </td>
                      <td
                        style={{
                          padding: "5px 0",
                          textAlign: "right",
                          whiteSpace: "nowrap",
                        }}
                      >
                        <span
                          style={{
                            display: "inline-flex",
                            gap: 3,
                            alignItems: "center",
                            flexWrap: "wrap",
                            justifyContent: "flex-end",
                          }}
                        >
                          {row.keys.map((k) => (
                            <kbd
                              key={`${row.action}-${k}`}
                              className="shortcut-key"
                            >
                              {k}
                            </kbd>
                          ))}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </div>
      </div>
    </ModalBackdrop>
  );
}
