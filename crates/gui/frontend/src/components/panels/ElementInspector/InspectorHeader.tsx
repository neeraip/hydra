import { PencilSquareIcon, XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";
import { TypeBadge } from "../../ui/TypeBadge";

// ── Inspector header ───────────────────────────────────────────────────────────

export function Header({
  id,
  subtitle,
  accentColor,
  onClose,
  onRename,
}: {
  id: string;
  /** The element type ("junction", "pipe", …) — rendered as the shared letter
   * badge with the full capitalised name beside it.
   *
   * That badge is the only mark of kind this header shows. It used to
   * carry a second one, hand-rolled by each caller: a dot for a node, a
   * stripe for a link, an outlined box for a region. All three were the
   * kind's colour and nothing else, so they looked like they said what
   * the badge says while saying strictly less, and each was free to
   * drift from the catalog on its own. */
  subtitle: string;
  accentColor: string;
  onClose: () => void;
  /** When provided, the ID becomes click-to-rename; called with the new ID
   * once the user commits a non-empty change. */
  onRename?: (newId: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(id);
  const [hover, setHover] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Reset when the selected element changes (or its id changes under us).
  useEffect(() => {
    setDraft(id);
    setEditing(false);
  }, [id]);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  function commit() {
    setEditing(false);
    const next = draft.trim();
    if (next && next !== id) onRename?.(next);
    else setDraft(id);
  }

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "10px 12px",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
      }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        {editing && onRename ? (
          <input
            ref={inputRef}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit();
              } else if (e.key === "Escape") {
                e.preventDefault();
                setDraft(id);
                setEditing(false);
              }
            }}
            style={{
              width: "100%",
              boxSizing: "border-box",
              fontSize: "var(--text-xl)",
              fontWeight: 600,
              color: "var(--text-primary)",
              background: "var(--bg-panel)",
              border: "1px solid var(--border-hover)",
              borderRadius: 4,
              padding: "1px 5px",
              outline: "none",
              fontFamily: "var(--font-ui)",
            }}
          />
        ) : onRename ? (
          <button
            type="button"
            onClick={() => setEditing(true)}
            onMouseEnter={() => setHover(true)}
            onMouseLeave={() => setHover(false)}
            data-tooltip="Rename"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              maxWidth: "100%",
              background: "transparent",
              border: "none",
              padding: "1px 0",
              margin: 0,
              cursor: "text",
              fontFamily: "var(--font-ui)",
            }}
          >
            <span
              style={{
                fontSize: "var(--text-xl)",
                fontWeight: 600,
                color: "var(--text-primary)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {id}
            </span>
            <PencilSquareIcon
              style={{
                width: 12,
                height: 12,
                flexShrink: 0,
                color: "var(--text-tertiary)",
                opacity: hover ? 1 : 0,
                transition: "opacity var(--t-fast)",
              }}
            />
          </button>
        ) : (
          <div
            style={{
              fontSize: "var(--text-xl)",
              fontWeight: 600,
              color: "var(--text-primary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {id}
          </div>
        )}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            marginTop: 2,
          }}
        >
          <TypeBadge type={subtitle} />
          <span
            style={{
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
              textTransform: "capitalize",
            }}
          >
            {subtitle}
          </span>
        </div>
      </div>
      <button
        type="button"
        onClick={onClose}
        data-tooltip="Close inspector"
        aria-label="Close inspector"
        style={{
          background: "transparent",
          border: "none",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          padding: 4,
          lineHeight: 1,
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <XMarkIcon style={{ width: 14, height: 14 }} />
      </button>
      {/* Hidden span keeps accentColor in the render tree for future use. */}
      <span style={{ display: "none" }}>{accentColor}</span>
    </div>
  );
}
