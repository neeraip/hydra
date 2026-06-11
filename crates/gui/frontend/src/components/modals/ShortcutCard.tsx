interface ShortcutRow {
  action: string;
  keys: string[];
}

interface ShortcutSection {
  title: string;
  rows: ShortcutRow[];
}

const SECTIONS: ShortcutSection[] = [
  {
    title: "Global",
    rows: [
      { action: "Command palette", keys: ["⌘K"] },
      { action: "Keyboard shortcuts", keys: ["?"] },
      { action: "Go to Overview", keys: ["⌘", "1"] },
      { action: "Go to Canvas", keys: ["⌘", "2"] },
      { action: "Go to Editor", keys: ["⌘", "3"] },
      { action: "Go to Analysis", keys: ["⌘", "4"] },
      { action: "Toggle sidebar", keys: ["⌘B"] },
      { action: "Settings", keys: ["⌘,"] },
    ],
  },
  {
    title: "Canvas",
    rows: [
      { action: "Pan", keys: ["Space", "drag"] },
      { action: "Zoom in/out", keys: ["Scroll"] },
      { action: "Select element", keys: ["Click"] },
      { action: "Multi-select", keys: ["⇧", "Click"] },
      { action: "Cycle variable", keys: ["Tab"] },
      { action: "Reset view", keys: ["⌘0"] },
      { action: "Toggle inspector", keys: ["I"] },
    ],
  },
  {
    title: "Playback",
    rows: [
      { action: "Play / Pause", keys: ["Space"] },
      { action: "Step forward", keys: ["→"] },
      { action: "Step backward", keys: ["←"] },
      { action: "Jump to start", keys: ["⌘←"] },
      { action: "Jump to end", keys: ["⌘→"] },
      { action: "Speed up", keys: ["+"] },
      { action: "Slow down", keys: ["-"] },
    ],
  },
  {
    title: "Editor",
    rows: [
      { action: "Save changes", keys: ["⌘S"] },
      { action: "New scenario", keys: ["⌥⌘N"] },
      { action: "Run simulation", keys: ["⌘R"] },
      { action: "Find element", keys: ["⌘F"] },
      { action: "Copy row", keys: ["⌘C"] },
      { action: "Undo", keys: ["⌘Z"] },
      { action: "Redo", keys: ["⌘⇧Z"] },
    ],
  },
];

export function ShortcutCard({ onClose }: { onClose: () => void }) {
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--bg-overlay)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 2000,
        animation: "fadeIn 120ms ease-out",
      }}
    >
      <div
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
            onClick={onClose}
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
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--nav-hover)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-tertiary)";
              (e.currentTarget as HTMLButtonElement).style.background =
                "transparent";
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
          {SECTIONS.map((section) => (
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
                          {row.keys.map((k, i) => (
                            <kbd key={i} className="shortcut-key">
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
    </div>
  );
}
