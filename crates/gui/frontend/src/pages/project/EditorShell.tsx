/**
 * The Editor page's skeleton, shared by every engine.
 *
 * The design concept is engine-neutral: a model is edited as a set of
 * *sections* listed down the left, one section visible at a time, with a
 * status bar across the foot. What a section contains — element tables,
 * curves, pollutants — is entirely the engine's business, and it supplies
 * those as content.
 *
 * This exists because the two engines' Editor pages had drifted into
 * different layouts: water distribution had the rail and the status bar,
 * drainage had neither and used a different tab primitive inside. Two
 * pages that do the same job should not have to be learned twice, and the
 * fix is a shared skeleton rather than a switch inside one page.
 *
 * Sections are given, never derived here — each engine decides what its
 * model is made of. An engine is expected to list every kind it declares,
 * including ones the loaded model has none of: that a model has no
 * pollutants is a fact worth reading, and hiding the entry made it
 * indistinguishable from the application being unable to show them.
 * Empty sections are drawn quieter rather than dropped.
 */

import { Fragment, type ReactNode } from "react";
import { TypeBadge } from "../../components/ui/TypeBadge";

/**
 * Fixed badge column, matching the network list's row.
 *
 * The badge is already a fixed width, so this does not stop labels going
 * ragged — it reserves the column for entries that carry no badge, so
 * their labels start on the same line as the rest instead of sliding left
 * by a badge's width.
 */
const BADGE_COL = 22;

export interface EditorSection {
  id: string;
  label: string;
  /** How many items the section holds; shown beside the label. */
  count: number;
  /** Unsaved changes staged in this section. `0` for read-only engines. */
  dirtyCount?: number;
  /** Element-kind id for the badge, when this section lists one. The same
   * badge the canvas and the inspector use, so a kind has one identity
   * everywhere it appears. Absent for sections that are not a kind. */
  kindId?: string;
  /** Starts a new group above this entry. Used to part the spatial kinds
   * from the collections without inventing a second level of navigation. */
  startsGroup?: boolean;
}

/** Amber, for staged-but-unsaved state. The one place this GUI uses colour
 * to mean "you have work in progress". */
const DIRTY = "rgba(220, 160, 40, 0.9)";

export function EditorShell({
  sections,
  activeSectionId,
  onSelectSection,
  footer,
  children,
}: {
  sections: readonly EditorSection[];
  activeSectionId: string;
  onSelectSection: (id: string) => void;
  /** The status bar's content. Engines with no editing pass a note saying
   * so, rather than an empty bar that looks like something failed. */
  footer?: ReactNode;
  /** All sections' bodies. Callers keep every one mounted and toggle
   * visibility, so per-section state survives switching. */
  children: ReactNode;
}) {
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
        animation: "fadeIn 150ms ease-out",
      }}
    >
      <div
        style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}
      >
        <nav
          aria-label="Editor sections"
          style={{
            width: 180,
            flexShrink: 0,
            background: "var(--bg-panel)",
            borderRight: "1px solid var(--border)",
            display: "flex",
            flexDirection: "column",
            overflow: "auto",
            paddingTop: 8,
          }}
        >
          {sections.map((s) => {
            const active = s.id === activeSectionId;
            // A kind the model has none of is still listed — that absence
            // is information — but it recedes so the kinds that do have
            // content are what the eye lands on. Never disabled: opening
            // it and reading "no elements of this kind" is the
            // confirmation the entry exists to give.
            const empty = s.count === 0;
            return (
              <Fragment key={s.id}>
                {/* The divider is its own element, not the row's top
                    border. As a border it was inside the button's box, so
                    the rule and the space above the label were part of the
                    click and hover target — the row lit up when the
                    pointer was over what looks like the gap between two
                    groups. */}
                {s.startsGroup && (
                  <hr
                    style={{
                      height: 1,
                      width: "auto",
                      margin: "8px 14px",
                      border: "none",
                      background: "var(--border)",
                      flexShrink: 0,
                    }}
                  />
                )}
                <button
                  type="button"
                  onClick={() => onSelectSection(s.id)}
                  aria-current={active ? "page" : undefined}
                  onMouseEnter={(e) => {
                    if (!active)
                      (e.currentTarget as HTMLButtonElement).style.background =
                        "var(--nav-hover)";
                  }}
                  onMouseLeave={(e) => {
                    if (!active)
                      (e.currentTarget as HTMLButtonElement).style.background =
                        "transparent";
                  }}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 8,
                    width: "100%",
                    // Longhands only. Mixing the `padding` shorthand with a
                    // conditional `paddingTop` silently broke this row:
                    // React writes style keys in order and assigns "" for
                    // undefined, so the later `paddingTop: undefined`
                    // *removed* the top padding the shorthand had set, and
                    // every non-group row rendered flush against its top
                    // edge. A shorthand and one of its longhands must never
                    // share an inline style object.
                    paddingTop: 8,
                    paddingBottom: 8,
                    paddingLeft: 14,
                    paddingRight: 14,
                    border: "none",
                    background: active ? "var(--accent-dim)" : "transparent",
                    borderLeft: active
                      ? "2px solid var(--accent)"
                      : "2px solid transparent",
                    color: active
                      ? "var(--text-primary)"
                      : "var(--text-secondary)",
                    cursor: "pointer",
                    fontSize: "var(--text-md)",
                    fontFamily: "var(--font-ui)",
                    textAlign: "left",
                    opacity: empty && !active ? 0.45 : 1,
                    transition:
                      "background var(--t-fast), opacity var(--t-fast)",
                  }}
                >
                  <span
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 8,
                      minWidth: 0,
                    }}
                  >
                    <span
                      style={{
                        width: BADGE_COL,
                        display: "flex",
                        justifyContent: "center",
                        flexShrink: 0,
                      }}
                    >
                      {s.kindId && <TypeBadge type={s.kindId} />}
                    </span>
                    <span
                      style={{
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {s.label}
                    </span>
                  </span>
                  <div
                    style={{ display: "flex", alignItems: "center", gap: 5 }}
                  >
                    {(s.dirtyCount ?? 0) > 0 && (
                      <span
                        role="img"
                        aria-label="unsaved changes"
                        style={{
                          width: 6,
                          height: 6,
                          borderRadius: "50%",
                          background: DIRTY,
                          display: "inline-block",
                          flexShrink: 0,
                        }}
                      />
                    )}
                    <span
                      style={{
                        fontSize: "var(--text-sm)",
                        fontFamily: "var(--font-mono)",
                        color: active
                          ? "var(--accent)"
                          : "var(--text-tertiary)",
                      }}
                    >
                      {s.count}
                    </span>
                  </div>
                </button>
              </Fragment>
            );
          })}
        </nav>

        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
            minHeight: 0,
          }}
        >
          {children}
        </div>
      </div>

      {footer}
    </div>
  );
}

/**
 * The status bar's resting state: a quiet line of text.
 *
 * Shared so an engine that stages no edits still gets the same bar in the
 * same place, rather than the page ending in a different silhouette.
 */
export function EditorStatusBar({
  tone = "quiet",
  children,
}: {
  /** `dirty` warms the bar to signal work in progress. */
  tone?: "quiet" | "dirty";
  children: ReactNode;
}) {
  const dirty = tone === "dirty";
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 16px",
        borderTop: `1px solid ${dirty ? "rgba(220, 160, 40, 0.3)" : "var(--border)"}`,
        flexShrink: 0,
        fontSize: "var(--text-md)",
        background: dirty ? "rgba(220, 160, 40, 0.07)" : undefined,
        transition: "background 200ms",
      }}
    >
      {children}
    </div>
  );
}
