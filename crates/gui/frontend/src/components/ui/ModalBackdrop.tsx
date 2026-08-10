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
 */

import type { CSSProperties, ReactNode, SyntheticEvent } from "react";
import { createPortal } from "react-dom";

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

export function ModalBackdrop({
  onDismiss,
  zIndex,
  background = "var(--bg-overlay)",
  style,
  children,
}: ModalBackdropProps) {
  const overlay = (
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the modal on pointer interaction.
    // biome-ignore lint/a11y/useKeyWithClickEvents: backdrop closes the modal on pointer interaction.
    <div
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
