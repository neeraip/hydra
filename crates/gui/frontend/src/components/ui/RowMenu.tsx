/**
 * RowMenu — an overflow ("…") menu of labelled actions for a table/list row.
 *
 * Exists because rows accumulate actions faster than they accumulate space.
 * A row of six bare icons forces the user to hover each one to find out what
 * it does, and puts a destructive action one pixel from a harmless one. The
 * frequent, safe actions stay inline; everything else moves in here, where
 * each entry carries a real label and the destructive ones sit apart.
 *
 * Items are shown rather than hidden when unavailable: a `disabled` entry
 * with a `disabledReason` tells the user the capability exists and why it
 * does not apply right now, which a missing entry cannot do.
 *
 * The menu renders through a portal on <body>. Row menus live inside
 * scrolling, `overflow: auto` containers that would otherwise clip the
 * dropdown to the row it belongs to.
 *
 * Dismissal listeners run in the CAPTURE phase, and opening claims a
 * process-wide exclusive slot (see `exclusiveOpen`). Both are needed because
 * these menus sit inside modal panels that call `stopPropagation` on
 * mousedown — a bubble-phase listener above them never sees the click at
 * all.
 */

import { EllipsisHorizontalIcon } from "@heroicons/react/16/solid";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { claimExclusive, releaseExclusive } from "./exclusiveOpen";

export interface RowMenuItem {
  /** Menu entry label. Also the accessible name. */
  label: string;
  onSelect: () => void;
  /** Renders in the destructive colour, in the group below the separator. */
  danger?: boolean;
  /** Renders in the caution colour, in the same group as `danger` entries.
   * For an action that removes or discards something, but only something the
   * user has already been told is not pulling its weight — worth marking,
   * short of the red reserved for losing work. */
  warning?: boolean;
  disabled?: boolean;
  /** Shown under the label when disabled — say why, not just that. */
  disabledReason?: string;
  /** Shown under the label regardless of state — a consequence worth knowing
   * before choosing, such as how much disk a clear reclaims. Suppressed while
   * `disabledReason` is showing, since an unavailable action has no
   * consequence to report. */
  detail?: string;
}

const MENU_WIDTH = 210;
const VIEWPORT_MARGIN = 8;
/** Distance between the trigger and the menu it opens. */
const TRIGGER_GAP = 4;

/**
 * Where the menu sits relative to its trigger.
 *
 * - `bottom-end` — below, right edges aligned. The default, for a menu at the
 *   end of a row where dropping downward is the obvious motion.
 * - `right-start` — beside, top edges aligned. For a trigger in a header
 *   above a list, where opening downward would cover the very rows the menu
 *   acts on.
 *
 * Both flip when the preferred side would leave the viewport.
 */
export type RowMenuPlacement = "bottom-end" | "right-start";

/** How the trigger presents itself.
 *
 * `ghost` is right in a list, where a menu sits on every row and drawing a
 * border on each would out-shout the content. `solid` is right when the menu
 * stands alone beside other buttons — a borderless control next to a bordered
 * one reads as secondary, or as not a button at all. */
export type RowMenuVariant = "ghost" | "solid";

export function RowMenu({
  items,
  label = "More actions",
  placement = "bottom-end",
  variant = "ghost",
}: {
  items: RowMenuItem[];
  /** Accessible name for the trigger. */
  label?: string;
  placement?: RowMenuPlacement;
  variant?: RowMenuVariant;
}) {
  const [open, setOpen] = useState(false);
  // Stable identity: `exclusiveOpen` recognises a holder by this reference,
  // so a per-render closure would strand the slot on unmount.
  const close = useCallback(() => setOpen(false), []);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  // Position against the trigger's viewport rect, flipping to the opposite
  // side when the preferred one would leave the window.
  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    const height = menuRef.current?.offsetHeight ?? 0;

    if (placement === "right-start") {
      const beside = rect.right + TRIGGER_GAP;
      const fits = beside + MENU_WIDTH <= window.innerWidth - VIEWPORT_MARGIN;
      setPos({
        // Top-aligned with the trigger, then pulled up only as far as
        // staying on screen requires.
        top: Math.max(
          VIEWPORT_MARGIN,
          Math.min(rect.top, window.innerHeight - height - VIEWPORT_MARGIN),
        ),
        left: fits
          ? beside
          : Math.max(VIEWPORT_MARGIN, rect.left - MENU_WIDTH - TRIGGER_GAP),
      });
      return;
    }

    const below = rect.bottom + TRIGGER_GAP;
    const flip = below + height > window.innerHeight - VIEWPORT_MARGIN;
    setPos({
      top: flip
        ? Math.max(VIEWPORT_MARGIN, rect.top - height - TRIGGER_GAP)
        : below,
      left: Math.min(
        rect.right - MENU_WIDTH,
        window.innerWidth - MENU_WIDTH - VIEWPORT_MARGIN,
      ),
    });
  }, [open, placement]);

  // Hold the exclusive slot for exactly as long as this menu is open, and
  // give it up on unmount so a closing row cannot block the next menu.
  useEffect(() => {
    if (!open) return;
    claimExclusive(close);
    return () => releaseExclusive(close);
  }, [open, close]);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: MouseEvent) {
      const target = e.target as Node;
      if (menuRef.current?.contains(target)) return;
      if (triggerRef.current?.contains(target)) return;
      setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
        triggerRef.current?.focus();
      }
    }
    // A scroll or resize moves the trigger out from under the menu; closing
    // beats re-measuring on every frame for something this short-lived.
    function onReflow() {
      setOpen(false);
    }
    // Capture phase: modal panels stop mousedown from bubbling, so a
    // bubble-phase listener here would never fire inside one.
    document.addEventListener("mousedown", onPointerDown, true);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("scroll", onReflow, true);
    window.addEventListener("resize", onReflow);
    return () => {
      document.removeEventListener("mousedown", onPointerDown, true);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("scroll", onReflow, true);
      window.removeEventListener("resize", onReflow);
    };
  }, [open]);

  // The separator opens the group of entries that take something away,
  // whether they are marked as caution or as destructive.
  const groupStart = items.findIndex((i) => i.danger || i.warning);

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        data-tooltip={open ? undefined : label}
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          width: 24,
          height: 24,
          border:
            variant === "solid"
              ? "1px solid var(--border)"
              : "1px solid transparent",
          borderRadius: 5,
          background: open
            ? "var(--nav-hover)"
            : variant === "solid"
              ? "var(--bg-elevated)"
              : "transparent",
          color: open ? "var(--text-primary)" : "var(--text-secondary)",
          cursor: "pointer",
          padding: 0,
          transition: "background var(--t-fast), color var(--t-fast)",
        }}
      >
        <EllipsisHorizontalIcon style={{ width: 14, height: 14 }} />
      </button>

      {open &&
        createPortal(
          <div
            ref={menuRef}
            role="menu"
            style={{
              position: "fixed",
              top: pos?.top ?? -9999,
              left: pos?.left ?? -9999,
              width: MENU_WIDTH,
              zIndex: 900,
              background: "var(--bg-card)",
              border: "1px solid var(--border)",
              borderRadius: 8,
              padding: 4,
              boxShadow: "0 12px 32px rgba(0,0,0,0.4)",
              backdropFilter: "blur(24px)",
              // Hidden until measured, so the flip decision never shows as a
              // jump from below the trigger to above it.
              visibility: pos ? "visible" : "hidden",
            }}
          >
            {items.map((item, i) => (
              <div key={item.label}>
                {/* Separator only when destructive entries actually follow
                    safe ones — a menu that opens with a danger item would
                    otherwise get a stray rule above its first row. */}
                {(item.danger || item.warning) && i === groupStart && i > 0 && (
                  <div
                    style={{
                      height: 1,
                      background: "var(--border)",
                      margin: "4px 0",
                    }}
                  />
                )}
                <button
                  type="button"
                  role="menuitem"
                  disabled={item.disabled}
                  onClick={() => {
                    setOpen(false);
                    item.onSelect();
                  }}
                  style={{
                    display: "block",
                    width: "100%",
                    textAlign: "left",
                    padding: "6px 9px",
                    border: "none",
                    borderRadius: 5,
                    background: "transparent",
                    color: item.disabled
                      ? "var(--text-tertiary)"
                      : item.danger
                        ? "var(--status-error)"
                        : item.warning
                          ? "var(--status-warning)"
                          : "var(--text-primary)",
                    fontSize: "var(--text-md)",
                    fontFamily: "var(--font-ui)",
                    cursor: item.disabled ? "default" : "pointer",
                    lineHeight: 1.45,
                  }}
                  onMouseEnter={(e) => {
                    if (!item.disabled) {
                      e.currentTarget.style.background = "var(--nav-hover)";
                    }
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "transparent";
                  }}
                >
                  {item.label}
                  {!item.disabled && item.detail && (
                    <span
                      style={{
                        display: "block",
                        fontSize: "var(--text-xs)",
                        color: "var(--text-tertiary)",
                        marginTop: 1,
                      }}
                    >
                      {item.detail}
                    </span>
                  )}
                  {item.disabled && item.disabledReason && (
                    <span
                      style={{
                        display: "block",
                        fontSize: "var(--text-xs)",
                        color: "var(--text-tertiary)",
                        marginTop: 1,
                      }}
                    >
                      {item.disabledReason}
                    </span>
                  )}
                </button>
              </div>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}
