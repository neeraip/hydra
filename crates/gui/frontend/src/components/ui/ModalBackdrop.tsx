/**
 * ModalBackdrop — shared full-screen dismissable backdrop for modal dialogs.
 *
 * Renders the fixed-inset, flex-centering overlay div that every modal
 * previously declared inline. Visual differences between modals (z-index,
 * background colour, entry animation, alignment) are passed through props;
 * the rendered DOM is a single <div>.
 *
 * # Why it portals to the body
 *
 * Every modal here used to be rendered by the app shell or a page root,
 * where "fixed, inset 0" means the window and nothing more is needed. The
 * licences panel is opened by a row *inside* the settings drawer — a
 * fixed, 680px-wide, scrolling column on the right of the window — and
 * inherited that column as its frame: the backdrop covered the drawer
 * rather than the window, and the centred panel had its left edge, its
 * title and its first tab outside the visible strip. It read as a card
 * stacked on the drawer with no way back to the tab it opened on.
 *
 * A full-window overlay must not be a descendant of whatever opened it,
 * because that thing may be scrolled, clipped, transformed or animated —
 * all of which redefine where "fixed" is fixed to. Portalling makes the
 * overlay a child of the body wherever it is written, so the modal reads
 * naturally at its trigger site and still belongs to the window.
 *
 * # Focus
 *
 * It also owns what a dialog owes the keyboard — focus in on open, Tab
 * kept inside, focus back to the opener on close (see `dialogFocus`).
 * That lives here rather than in each modal because every modal already
 * renders one of these, and thirteen hand-written copies of a focus trap
 * is thirteen chances to write twelve of them.
 */

import type { CSSProperties, ReactNode, SyntheticEvent } from "react";
import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { initialFocus, nextFocus, restoreFocus } from "./dialogFocus";

/**
 * Spread onto a modal's panel element so pointer/keyboard events inside the
 * panel never bubble up to the backdrop's dismiss handler.
 */
export const stopBackdropEvents = {
  onMouseDown: (e: SyntheticEvent) => e.stopPropagation(),
  onKeyDown: (e: SyntheticEvent) => e.stopPropagation(),
  onClick: (e: SyntheticEvent) => e.stopPropagation(),
};

interface ModalBackdropProps {
  /** Called when the backdrop is clicked. Panels spread `stopBackdropEvents`
   *  so clicks inside them never reach this handler. Omit for modals that
   *  only close via an explicit control. */
  onDismiss?: () => void;
  zIndex: number;
  background?: string;
  /** Extra declarations merged over the base backdrop style
   *  (e.g. entry animation or alignment overrides). */
  style?: CSSProperties;
  children: ReactNode;
}

/**
 * The stack of open backdrops, innermost last.
 *
 * Only the top one may hold focus: two open at once — a confirmation over
 * a panel — would otherwise both pull Tab back to themselves, and the
 * reader would find the ring bouncing between two dialogs.
 */
const openBackdrops: HTMLElement[] = [];

export function ModalBackdrop({
  onDismiss,
  zIndex,
  background = "var(--bg-overlay)",
  style,
  children,
}: ModalBackdropProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = ref.current;
    if (!container) return;
    // Captured before focus moves anywhere, so what is restored is the
    // control that opened this — not whatever the dialog focused first.
    const opener = document.activeElement;
    openBackdrops.push(container);

    initialFocus(container, document.activeElement)?.focus();

    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Tab") return;
      if (openBackdrops[openBackdrops.length - 1] !== container) return;
      const next = nextFocus(
        container as HTMLElement,
        document.activeElement,
        e.shiftKey,
      );
      if (next) {
        e.preventDefault();
        next.focus();
      }
    }
    // Capture, so the wrap happens before any handler inside the dialog
    // sees the key and regardless of where focus currently is.
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      const at = openBackdrops.indexOf(container);
      if (at !== -1) openBackdrops.splice(at, 1);
      restoreFocus(opener);
    };
  }, []);

  const overlay = (
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the modal on pointer interaction.
    // biome-ignore lint/a11y/useKeyWithClickEvents: backdrop closes the modal on pointer interaction.
    <div
      ref={ref}
      onClick={onDismiss}
      style={{
        position: "fixed",
        inset: 0,
        background,
        zIndex,
        display: "flex",
        // `safe` centring: a child taller than the window overflows *both*
        // ways under plain `center`, putting its head above the top edge
        // where nothing can scroll to it. `safe` falls back to start
        // alignment exactly when that would happen, so a tall modal scrolls
        // from its top and a short one stays centred.
        alignItems: "safe center",
        justifyContent: "safe center",
        overflow: "auto",
        ...style,
      }}
    >
      {children}
    </div>
  );
  return createPortal(overlay, document.body);
}
