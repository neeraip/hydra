// ── Undo and redo, with the history behind them ──────────────────────────────
//
// The stacks were reachable by ⌘Z, by the command palette, and by the
// shortcut card — three ways in, all of them things you had to already
// know about. Nothing on screen said a history existed, and the palette
// listed Undo whether or not there was anything to undo, so even finding
// it told you nothing. Under write-through editing that matters more than
// it would elsewhere: there is no save step, every edit is committed as it
// is made, and the only way back was a history with no surface.
//
// **At the right end of the bar, not beside the navigation arrows.** Back
// and forward move between screens; these change the model. Two pairs of
// arrows a hundred pixels apart, one of which edits your network, is a
// mistake the layout would be inviting — so they sit at the other end and
// wear the turning glyphs rather than plain ones.
//
// The menus list and do not jump. Walking back three entries means
// applying three inverses in order, any of which may be refused by a
// model that has moved on, and stopping halfway is the "partly done,
// reported as done" outcome this codebase makes unrepresentable
// everywhere else. Reading the history is the part that was missing.

import {
  ArrowUturnLeftIcon,
  ArrowUturnRightIcon,
} from "@heroicons/react/24/outline";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { useElementKinds } from "../../hooks/engines";
import { useUndoStacks } from "../../hooks/undoStack";
import { useUndoRedo } from "../../hooks/useUndoRedo";
import { clampedMenuLeft } from "../ui/menuPlacement";
import { TypeBadge } from "../ui/TypeBadge";
import { historyMenu, historyTooltip, nextEntry } from "./history";

export function HistoryControls() {
  const { page, activeScenarioId } = useAppState();
  const { project } = useActiveProject();
  // The engine's own word for each kind, for the one place this
  // interface names a kind in words — see `historyTooltip`.
  const kinds = useElementKinds(project?.engine);
  const kindLabel = (id: string) => kinds.find((k) => k.id === id)?.label;
  const { undo, redo } = useUndoRedo();
  const stacks = useUndoStacks(project?.id ?? null, activeScenarioId ?? null);
  const [open, setOpen] = useState<"undo" | "redo" | null>(null);

  // Absent off the project page rather than disabled, matching ⌘Z, which
  // is gated the same way. A history belongs to a project and a scenario,
  // so on the Home or Settings page there is not an empty one — there is
  // no question being asked.
  if (page !== "project" || !project) return null;

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
      <HistoryButton
        title="Undo"
        tooltip={historyTooltip("Undo", nextEntry(stacks.undo), kindLabel)}
        empty={stacks.undo.length === 0}
        menu={historyMenu(stacks.undo)}
        open={open === "undo"}
        onToggle={() => setOpen((o) => (o === "undo" ? null : "undo"))}
        onClose={() => setOpen(null)}
        onApply={undo}
      >
        <ArrowUturnLeftIcon style={{ width: 14, height: 14 }} />
      </HistoryButton>
      <HistoryButton
        title="Redo"
        tooltip={historyTooltip("Redo", nextEntry(stacks.redo), kindLabel)}
        empty={stacks.redo.length === 0}
        menu={historyMenu(stacks.redo)}
        open={open === "redo"}
        onToggle={() => setOpen((o) => (o === "redo" ? null : "redo"))}
        onClose={() => setOpen(null)}
        onApply={redo}
      >
        <ArrowUturnRightIcon style={{ width: 14, height: 14 }} />
      </HistoryButton>
    </div>
  );
}

function HistoryButton({
  title,
  tooltip,
  empty,
  menu,
  open,
  onToggle,
  onClose,
  onApply,
  children,
}: {
  title: string;
  /** What the button promises, already composed. */
  tooltip: string;
  /** Whether there is anything to apply. */
  empty: boolean;
  menu: ReturnType<typeof historyMenu>;
  open: boolean;
  onToggle: () => void;
  onClose: () => void;
  onApply: () => void;
  children: React.ReactNode;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // Measured, then moved, before the browser paints. Anchoring in CSS
  // alone is what put this menu off the right of the window: a static
  // side is right until the window is narrow enough that it is not, and
  // no rule written in CSS can see the viewport.
  useLayoutEffect(() => {
    if (!open) return;
    const el = menuRef.current;
    const anchor = wrap.current?.getBoundingClientRect();
    if (!el || !anchor) return;
    el.style.left = `${clampedMenuLeft(
      anchor.right,
      el.getBoundingClientRect().width,
      window.innerWidth,
    )}px`;
    el.style.top = `${anchor.bottom + 4}px`;
  }, [open]);

  // Dismissed by clicking away, like every other transient surface here.
  // Bound only while open, so a closed menu costs no listener.
  useEffect(() => {
    if (!open) return;
    function away(e: MouseEvent) {
      if (!wrap.current?.contains(e.target as globalThis.Node)) onClose();
    }
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open, onClose]);

  return (
    <div ref={wrap} style={{ position: "relative", display: "flex" }}>
      <button
        type="button"
        // The tooltip names the edit rather than the control: "Undo" over
        // a live button says nothing the icon has not already said, and
        // over a dead one it promises something that will not happen.
        data-tooltip={tooltip}
        data-tooltip-pos="bottom"
        aria-label={title}
        onClick={onApply}
        disabled={empty}
        style={buttonStyle(empty, {
          borderTopRightRadius: 0,
          borderBottomRightRadius: 0,
        })}
        onMouseEnter={(e) => hover(e, empty, true)}
        onMouseLeave={(e) => hover(e, empty, false)}
      >
        {children}
      </button>
      <button
        type="button"
        data-tooltip={`${title} history`}
        data-tooltip-pos="bottom"
        aria-label={`${title} history`}
        aria-expanded={open}
        onClick={onToggle}
        disabled={empty}
        style={{
          ...buttonStyle(empty, {
            borderTopLeftRadius: 0,
            borderBottomLeftRadius: 0,
          }),
          width: 14,
        }}
        onMouseEnter={(e) => hover(e, empty, true)}
        onMouseLeave={(e) => hover(e, empty, false)}
      >
        <Caret />
      </button>

      {open && !empty && (
        <div
          ref={menuRef}
          style={{
            // Fixed rather than absolute, so the placement is decided in
            // viewport coordinates — which is the only space in which
            // "does this fit" is a question that can be answered.
            position: "fixed",
            // Offscreen until measured; the layout effect above moves it
            // before the paint.
            left: -9999,
            top: -9999,
            minWidth: 200,
            maxWidth: 320,
            background: "var(--bg-panel)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            boxShadow: "0 6px 20px rgba(0,0,0,0.35)",
            padding: "4px 0",
            zIndex: 60,
          }}
        >
          {menu.items.map((item, i) => (
            // Keyed by position: two identical edits are two entries, and
            // a key built from the label would collapse them into one.
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: entries are positional
              key={i}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "4px 10px",
                fontSize: "var(--text-md)",
                fontFamily: "var(--font-ui)",
                // The first is the one the button applies; the rest are
                // what is behind it. Dimming them says which is which
                // without claiming any of them can be clicked.
                color: i === 0 ? "var(--text-primary)" : "var(--text-tertiary)",
                whiteSpace: "nowrap",
              }}
            >
              {/* The kind's glyph, never its name — and never absent
                  where the capture knew it. "Changed invert on 9" names
                  half an element, because an id is unique only within
                  its class: a junction 9 and a conduit 9 are two things
                  sharing a name, and deciding whether to undo means
                  knowing which one it happened to. */}
              {item.subject && <TypeBadge type={item.subject.kind} size="sm" />}
              <span
                style={{
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {item.label}
              </span>
            </div>
          ))}
          {/* Counted, never dropped in silence: a list that stopped at ten
              and said nothing would read as the whole history. */}
          {menu.more > 0 && (
            <div
              style={{
                padding: "4px 10px",
                fontSize: "var(--text-sm)",
                color: "var(--text-disabled)",
                borderTop: "1px solid var(--border)",
                marginTop: 4,
              }}
            >
              {menu.more} older
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function buttonStyle(
  disabled: boolean,
  radii: React.CSSProperties,
): React.CSSProperties {
  return {
    width: 28,
    height: 28,
    borderRadius: 5,
    background: "transparent",
    border: "1px solid transparent",
    color: disabled ? "var(--text-disabled)" : "var(--text-secondary)",
    cursor: disabled ? "not-allowed" : "pointer",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    transition: "background var(--t-fast), border-color var(--t-fast)",
    ...radii,
  };
}

function hover(
  e: React.MouseEvent<HTMLButtonElement>,
  disabled: boolean,
  on: boolean,
) {
  if (disabled) return;
  e.currentTarget.style.background = on ? "var(--bg-card)" : "transparent";
}

function Caret() {
  return (
    <svg width="8" height="8" viewBox="0 0 8 8" aria-hidden="true">
      <path
        d="M1 3 L4 6 L7 3"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
    </svg>
  );
}
