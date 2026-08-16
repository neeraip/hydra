import { PROJECT_VIEWS, VIEW_SHORTCUTS } from "../../projectConfig";
import { primaryModifierLabel, shiftModifierLabel } from "../../shortcuts";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

export interface ShortcutRow {
  action: string;
  keys: string[];
}

export interface ShortcutSection {
  title: string;
  rows: ShortcutRow[];
}

/**
 * One row per view the number keys reach, named as the app names it.
 *
 * In digit order rather than catalog order, because the reader is
 * scanning the keys. A view with no number key gets no row: the card
 * says what the app listens for, and inventing a row for the Report view
 * would be the drift this function exists to remove, pointing the other
 * way.
 */
export function viewRows(modifier: string): ShortcutRow[] {
  return Object.entries(VIEW_SHORTCUTS)
    .sort(([a], [b]) => a.localeCompare(b))
    .flatMap(([digit, view]) => {
      const spec = PROJECT_VIEWS.find((v) => v.id === view);
      return spec
        ? [{ action: `Go to ${spec.label}`, keys: [modifier, digit] }]
        : [];
    });
}

/**
 * Everything the card claims this app listens for.
 *
 * Separated from the component so it can be checked. It is a hand-written
 * list beside a hand-written switch of key handlers, which is a pairing
 * that drifts — and the drift is invisible, because a card is only ever
 * read by someone who does not already know the answer. The rows that can
 * be derived from what the app actually routes with now are, which is the
 * only fix for that pairing rather than a warning about it.
 *
 * The modifier labels are parameters rather than being read here, so a test
 * does not depend on which platform it runs on.
 */
export function shortcutSections(
  modifier: string,
  shift: string,
): ShortcutSection[] {
  const sections: ShortcutSection[] = [
    {
      title: "Global",
      rows: [
        { action: "Command palette", keys: [modifier, "K"] },
        { action: "Settings", keys: [modifier, ","] },
        { action: "Run simulation", keys: [modifier, "R"] },
        // No save row. An edit is part of the model when the operation
        // returns (hydra-common §4.5.5), so there is nothing to save and
        // ⌘S is swallowed only to keep the browser's own dialog away.
        // The card listed it for as long as the staged editors existed
        // and for a while after they were deleted.
        { action: "Undo network edit", keys: [modifier, "Z"] },
        { action: "Redo network edit", keys: [modifier, shift, "Z"] },
        // One key, one meaning, two things to search: the projects list
        // searches projects, an open project searches its elements. Two
        // rows showing the same keys read as a clash, which is what this
        // looked like before — they are the same shortcut doing the
        // obvious thing wherever you are.
        {
          action: "Search — projects, or elements in a project",
          keys: [modifier, "F"],
        },
        { action: "Toggle geographic/orthogonal", keys: [modifier, "M"] },
        { action: "Zoom in", keys: [modifier, "="] },
        { action: "Zoom out", keys: [modifier, "-"] },
        { action: "Fit network", keys: [modifier, "0"] },
        { action: "Toggle issues panel", keys: [modifier, shift, "M"] },
        { action: "Keyboard shortcuts", keys: ["?"] },
        // Built from the map the key handler routes with and the labels
        // the activity bar draws, so a view cannot be listed under a name
        // it no longer answers to. It was: ⌘4 was captioned "Go to
        // Analysis" after that view had been relabelled "Results".
        ...viewRows(modifier),
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

  return sections;
}

export function ShortcutCard({ onClose }: { onClose: () => void }) {
  const modifier = primaryModifierLabel();
  const shift = shiftModifierLabel();
  const sections = shortcutSections(modifier, shift);

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
              fontSize: "var(--text-2xl)",
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
              fontSize: "var(--text-2xl)",
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
                  fontSize: "var(--text-sm)",
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
                          fontSize: "var(--text-lg)",
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
