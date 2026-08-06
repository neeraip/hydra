/**
 * Settings, as a drawer over whatever is underneath.
 *
 * It was a page, and being a page cost it twice: it joined navigation
 * history, so Back walked through settings visits, and arriving at it
 * counted as leaving your project, which erased what the next launch would
 * have reopened. Both followed from calling a detour a destination. As an
 * overlay the page beneath is untouched — open it mid-project, change a
 * unit, dismiss, and you are exactly where you were.
 *
 * # Why the shell is eager and only the body is split
 *
 * The whole drawer was code-split at first, chrome included, so *nothing*
 * rendered until the chunk resolved: the click appeared to do nothing for
 * as long as the load took. That is worse than the page it replaced, where
 * the app shell was already on screen and only the content was pending —
 * the wait had somewhere to happen.
 *
 * So the split moved. This file holds the backdrop, the panel and the
 * header, costs almost nothing, and is imported eagerly; the rows behind it
 * are the part worth splitting and stream into an already-open drawer. The
 * drawer opens on the click that asked for it, every time, and the content
 * arrives where the user is already looking.
 */

import { XMarkIcon } from "@heroicons/react/16/solid";
import type React from "react";
import { lazy, Suspense, useEffect } from "react";
import { useAppState } from "../../AppContext";
import { loadSettingsContent } from "../../lazyChunks";
import { Spinner } from "../ui/Spinner";

const SettingsContent = lazy(() =>
  loadSettingsContent().then((m) => ({ default: m.SettingsContent })),
);

/**
 * The drawer's inner column.
 *
 * `width: 100%` is load-bearing, not belt-and-braces. The panel is a
 * column flex container, and auto cross-axis margins on a flex item
 * suppress the default stretch — so without a width this box sized to its
 * *content*: narrow while the spinner was all it held, then widening to
 * the 680 cap as the rows arrived, dragging the header out with it.
 *
 * Exported so `SettingsDrawer.layout.test.tsx` asserts against this object
 * rather than a copy of it — the failure it guards against is precisely
 * someone deleting the width as redundant.
 */
export const SETTINGS_COLUMN: React.CSSProperties = {
  width: "100%",
  maxWidth: 680,
  margin: "0 auto",
  padding: "40px 44px",
};

export function SettingsDrawer() {
  const { settingsOpen, closeSettings } = useAppState();

  // Close on Escape, as every other modal in the app does. Lives here
  // rather than in the content so it works during the moment the content
  // is still loading — a drawer you cannot dismiss yet is worse than one
  // that has not finished filling in.
  useEffect(() => {
    if (!settingsOpen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") closeSettings();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [settingsOpen, closeSettings]);

  if (!settingsOpen) return null;
  return (
    /* No backdrop, and not `aria-modal`: the drawer covers the right of
       the window and everything it does not cover stays live — the canvas
       still pans, the activity bar still switches, and a setting can be
       changed while watching what it does.
    
       That also means no dismiss-on-outside-click. A click outside a
       non-modal panel belongs to whatever it landed on; stealing it to
       close the drawer would make the rest of the window a dismiss target
       rather than the app. Escape and the close button remain. */
    <div
      role="dialog"
      aria-label="Settings"
      style={{
        position: "fixed",
        top: 0,
        right: 0,
        bottom: 0,
        zIndex: 200,
        display: "flex",
      }}
    >
      <div
        style={{
          width: "min(680px, 100vw)",
          height: "100%",
          background: "var(--bg-app)",
          borderLeft: "1px solid var(--border)",
          boxShadow: "var(--shadow-3)",
          display: "flex",
          flexDirection: "column",
          // `scroll`, not `auto`: the skeleton is shorter than the content
          // it stands in for, so an `auto` scrollbar appeared only once the
          // real rows landed — taking width from the column and re-wrapping
          // every description at the moment the jump was meant to be gone.
          // Reserving the track costs nothing where scrollbars are overlays.
          overflowY: "scroll",
          animation: "slideInRight 180ms ease-out",
        }}
      >
        <div style={SETTINGS_COLUMN}>
          {/* The header is chrome, not content: it names the drawer and
              offers the way out, both of which have to be there from the
              first frame rather than after a load. */}
          <div
            style={{
              display: "flex",
              alignItems: "flex-start",
              justifyContent: "space-between",
              gap: 16,
            }}
          >
            <h1
              style={{
                margin: "0 0 4px",
                fontSize: "var(--text-3xl)",
                fontWeight: 700,
                letterSpacing: "-0.015em",
              }}
            >
              Settings
            </h1>
            <button
              type="button"
              onClick={closeSettings}
              aria-label="Close settings"
              data-tooltip="Close"
              style={{
                flexShrink: 0,
                marginTop: 6,
                width: 28,
                height: 28,
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: "transparent",
                color: "var(--text-secondary)",
                cursor: "pointer",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "var(--nav-hover)";
                e.currentTarget.style.color = "var(--text-primary)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "var(--text-secondary)";
              }}
            >
              <XMarkIcon style={{ width: 15, height: 15 }} />
            </button>
          </div>
          <p
            style={{
              margin: "0 0 4px",
              color: "var(--text-secondary)",
              fontSize: "var(--text-xl)",
            }}
          >
            Appearance, accessibility, and maintenance tools.
          </p>
          {/* A spinner rather than a skeleton of the rows.

              A skeleton has to mirror what it stands in for, and the
              mirroring is what kept failing: every row it got wrong by a
              control's width or a description's wrap put the layout back
              where it started — moving. Maintaining that likeness by hand
              buys stillness only while it stays exactly right, and it did
              not.

              So the loading state stops pretending to be the content. It
              says it is loading, in a reserved block, and the content
              fades in — nothing claims to be in the right place before it
              is. */}
          <Suspense fallback={<SettingsLoading />}>
            <div style={{ animation: "fadeIn 200ms ease-out" }}>
              <SettingsContent />
            </div>
          </Suspense>
        </div>
      </div>
    </div>
  );
}

/** The reserved block the rows arrive into. Tall enough that the drawer
 *  does not look empty, and centred so nothing sits where a row will. */
function SettingsLoading() {
  return (
    <div
      style={{
        minHeight: 280,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 10,
        color: "var(--text-tertiary)",
        fontSize: "var(--text-lg)",
      }}
    >
      <Spinner />
      Loading…
    </div>
  );
}
