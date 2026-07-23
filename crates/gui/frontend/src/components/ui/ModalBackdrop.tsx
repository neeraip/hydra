/**
 * ModalBackdrop — shared full-screen dismissable backdrop for modal dialogs.
 *
 * Renders the fixed-inset, flex-centering overlay div that every modal
 * previously declared inline. Visual differences between modals (z-index,
 * background colour, entry animation, alignment) are passed through props;
 * the rendered DOM is a single <div>, identical to the former inline markup.
 */

import type { CSSProperties, ReactNode, SyntheticEvent } from "react";

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
  return (
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
        alignItems: "center",
        justifyContent: "center",
        ...style,
      }}
    >
      {children}
    </div>
  );
}
