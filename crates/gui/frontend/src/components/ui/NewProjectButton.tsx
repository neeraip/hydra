/**
 * "New project", with the other ways to start one behind a caret.
 *
 * The main action is the ordinary flow — choose an engine, then a file (or
 * no file). The menu is for the reader who already has a model and would
 * rather begin from it: the file itself names its engine through the
 * recognition contract, so nothing needs to be chosen before opening it.
 *
 * A split button rather than two: they begin the same thing, and the
 * difference between them is what you happen to have in your hand. The
 * menu grows one entry per way in — a second lands when the open-channel
 * engine ships.
 */

import { ChevronDownIcon } from "@heroicons/react/16/solid";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  formatInpImportError,
  type ImportedModel,
  openAndRecogniseNetwork,
} from "../../hooks";
import { clampedMenuLeft } from "./menuPlacement";
import { PrimaryButton } from "./PrimaryButton";

export function NewProjectButton({
  size,
  onNew,
  onImported,
  onArchive,
  onError,
}: {
  size?: "sm";
  /** The main action: open the wizard with nothing chosen. */
  onNew: () => void;
  /** A model was read and recognised; open the wizard on it. */
  onImported: (model: ImportedModel) => void;
  /** The user chose "Import archive": the page owns the picker and the
   * review flow, because they outlive this menu. */
  onArchive: () => void;
  /** Why a chosen file could not be opened — including the deliberate
   * refusal to guess when no engine claims it (hydra-common §2.5.1). */
  onError: (message: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  // Where to draw the menu, measured from the caret when it opens.
  //
  // Fixed rather than absolute, for the reason the unit picker's menu and
  // the criteria popover are: this button sits in a column with
  // `overflow: hidden`, which clips any child extending past it however
  // high the z-index goes — the menu was being cut off at the rail's edge.
  // Its right edge and its top, in viewport coordinates. The *left* is
  // computed once the menu has rendered and can be measured, because the
  // number that decides whether it fits is the width it actually took
  // rather than the `minWidth` it declared — a menu whose longest item is
  // wider than its minimum ran off the window while the arithmetic said
  // it fitted.
  const [anchor, setAnchor] = useState<{ right: number; top: number } | null>(
    null,
  );
  const menuRef = useRef<HTMLDivElement>(null);

  // Measured, then moved — before the browser paints, so the menu is
  // never seen at the offscreen position it renders at first. Applied to
  // the node rather than held in state: a state round trip would be a
  // second render the reader could catch.
  useLayoutEffect(() => {
    const el = menuRef.current;
    if (!el || !anchor) return;
    el.style.left = `${clampedMenuLeft(
      anchor.right,
      el.getBoundingClientRect().width,
      window.innerWidth,
    )}px`;
  }, [anchor]);

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    // The anchor is measured, not tracked: closing is the honest response
    // to the page moving under it.
    const close = () => setOpen(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [open]);

  async function importAndOpen() {
    setOpen(false);
    if (busy) return;
    setBusy(true);
    try {
      const model = await openAndRecogniseNetwork();
      // Null is a cancelled dialog, which is not a failure and says
      // nothing to report.
      if (model) onImported(model);
    } catch (e) {
      onError(formatInpImportError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div ref={wrapRef} style={{ position: "relative", flexShrink: 0 }}>
      <div style={{ display: "inline-flex", alignItems: "stretch" }}>
        <PrimaryButton
          size={size}
          onClick={onNew}
          style={{ borderTopRightRadius: 0, borderBottomRightRadius: 0 }}
        >
          + New project
        </PrimaryButton>
        <PrimaryButton
          size={size}
          onClick={() => {
            // Right-aligned to the split button: the menu is wider than
            // the button, and hanging it off the left edge pushed it into
            // whatever sits to the right. Measured from the wrapper, which
            // is the whole button rather than the caret it opened from.
            const wrap = wrapRef.current?.getBoundingClientRect();
            if (wrap) {
              setAnchor({ right: wrap.right, top: wrap.bottom + 4 });
            }
            setOpen((v) => !v);
          }}
          aria-label="Other ways to start a project"
          aria-haspopup="menu"
          aria-expanded={open}
          data-tooltip="Other ways to start"
          data-tooltip-pos="bottom"
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "0 6px",
            borderTopLeftRadius: 0,
            borderBottomLeftRadius: 0,
            // Parts the halves in the button's own text colour, dimmed —
            // the same seam the Simulate split button uses.
            boxShadow: "inset 1px 0 0 rgba(0,0,0,0.18)",
          }}
        >
          <ChevronDownIcon style={{ width: 12, height: 12 }} />
        </PrimaryButton>
      </div>
      {open && anchor && (
        <div
          role="menu"
          ref={menuRef}
          style={{
            position: "fixed",
            top: anchor.top,
            // Placed offscreen until it has been measured, then moved by
            // the layout effect below — the same order `TooltipPortal`
            // uses, so the reader never sees it in the wrong place.
            left: -9999,
            // The app's overlay band, shared with the toolbar's popovers:
            // above the cards and the rail this menu is drawn over.
            zIndex: 120,
            minWidth: 260,
            padding: "4px 0",
            borderRadius: 8,
            border: "1px solid var(--border)",
            background: "var(--bg-panel)",
            boxShadow: "var(--shadow-2)",
          }}
        >
          <button
            type="button"
            role="menuitem"
            disabled={busy}
            onClick={importAndOpen}
            className="legend-picker-option"
          >
            {busy ? "Opening…" : "Import from EPANET/SWMM (.inp)"}
          </button>
          <button
            type="button"
            role="menuitem"
            disabled={busy}
            onClick={() => {
              setOpen(false);
              onArchive();
            }}
            className="legend-picker-option"
          >
            Import archive of models (.zip, .7z, .tar)
          </button>
        </div>
      )}
    </div>
  );
}
