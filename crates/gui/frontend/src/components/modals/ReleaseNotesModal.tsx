import {
  ArrowTopRightOnSquareIcon,
  ChevronDownIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  ALL_RELEASES_URL,
  compareSemver,
  defaultExpandedVersions,
  type GuiRelease,
  releaseHasNotes,
} from "../../hooks/useReleaseNotes";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

/** Full-body markdown component map: normal document rendering (headings,
 * lists, tables via remark-gfm, code, images) with app typography; links
 * open externally through the opener plugin. Raw HTML stays disabled —
 * react-markdown's default. */
const MODAL_COMPONENTS: Components = {
  a: ({ href, children }) => (
    <button
      type="button"
      onClick={() => {
        if (href) void openUrl(href);
      }}
      style={{
        background: "transparent",
        border: "none",
        padding: 0,
        color: "var(--accent)",
        cursor: "pointer",
        fontSize: "inherit",
        fontFamily: "inherit",
        textDecoration: "underline",
        textUnderlineOffset: 2,
      }}
    >
      {children}
    </button>
  ),
  h1: ({ children }) => <div style={headingStyle(14)}>{children}</div>,
  h2: ({ children }) => <div style={headingStyle(13)}>{children}</div>,
  h3: ({ children }) => <div style={headingStyle(12.5)}>{children}</div>,
  h4: ({ children }) => <div style={headingStyle(12)}>{children}</div>,
  h5: ({ children }) => <div style={headingStyle(12)}>{children}</div>,
  h6: ({ children }) => <div style={headingStyle(12)}>{children}</div>,
  p: ({ children }) => (
    <p style={{ margin: "0 0 8px", fontSize: 12.5, lineHeight: 1.6 }}>
      {children}
    </p>
  ),
  ul: ({ children }) => (
    <ul style={{ margin: "0 0 8px", paddingLeft: 18 }}>{children}</ul>
  ),
  ol: ({ children }) => (
    <ol style={{ margin: "0 0 8px", paddingLeft: 18 }}>{children}</ol>
  ),
  li: ({ children }) => (
    <li style={{ fontSize: 12.5, lineHeight: 1.6, marginBottom: 2 }}>
      {children}
    </li>
  ),
  code: ({ children }) => (
    <code
      style={{
        fontFamily: "var(--font-mono)",
        fontSize: 11.5,
        background: "var(--bg-input, rgba(127,127,127,0.12))",
        borderRadius: 3,
        padding: "1px 4px",
      }}
    >
      {children}
    </code>
  ),
  pre: ({ children }) => (
    <pre
      style={{
        margin: "0 0 8px",
        padding: "8px 10px",
        background: "var(--bg-input, rgba(127,127,127,0.12))",
        border: "1px solid var(--border)",
        borderRadius: 6,
        overflowX: "auto",
        fontSize: 11.5,
        lineHeight: 1.5,
      }}
    >
      {children}
    </pre>
  ),
  img: ({ src, alt }) => (
    <img
      src={typeof src === "string" ? src : undefined}
      alt={alt ?? ""}
      style={{ maxWidth: "100%", borderRadius: 6, margin: "4px 0 8px" }}
    />
  ),
  hr: () => (
    <div style={{ height: 1, background: "var(--border)", margin: "10px 0" }} />
  ),
};

function headingStyle(fontSize: number): React.CSSProperties {
  return {
    fontSize,
    fontWeight: 700,
    color: "var(--text-primary)",
    margin: "10px 0 6px",
    lineHeight: 1.4,
  };
}

/** Accent "New" pill shown on releases newer than the last-seen marker. */
function NewBadge() {
  return (
    <span
      style={{
        fontSize: 9,
        fontWeight: 700,
        letterSpacing: "0.07em",
        textTransform: "uppercase",
        color: "var(--accent)",
        background: "var(--accent-dim)",
        borderRadius: 4,
        padding: "1px 5px",
        flexShrink: 0,
      }}
    >
      New
    </span>
  );
}

/** Small ↗ button opening a release's GitHub page — a sibling of the
 * accordion toggle (never nested inside it), so following the link cannot
 * toggle the item. */
function OpenReleaseButton({ url }: { url: string }) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        void openUrl(url);
      }}
      aria-label="Open this release on GitHub"
      data-tooltip="Open this release on GitHub"
      data-tooltip-pos="bottom"
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: 22,
        height: 22,
        border: "none",
        borderRadius: 4,
        background: "transparent",
        color: "var(--text-tertiary)",
        cursor: "pointer",
        padding: 0,
        flexShrink: 0,
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.color = "var(--accent)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.color =
          "var(--text-tertiary)";
      }}
    >
      <ArrowTopRightOnSquareIcon style={{ width: 12, height: 12 }} />
    </button>
  );
}

/**
 * Release-notes modal: ALL fetched GUI releases newest-first as an accordion.
 * Releases strictly newer than the last-seen marker start expanded and carry
 * a "New" badge (the same partition unseenReleases implements); plumbing-only
 * releases render as a compact muted one-liner with nothing to expand. The
 * caller advances the last-seen marker when this closes.
 */
export function ReleaseNotesModal({
  releases,
  lastSeen,
  onClose,
}: {
  /** Every fetched release, newest-first (never empty when rendered). */
  releases: GuiRelease[];
  /** Last-seen GUI version marker; null = unseeded (nothing unseen). */
  lastSeen: string | null;
  onClose: () => void;
}) {
  // Default expansion: strictly newer than the marker start open, plus the
  // newest release even when already seen; the rest start collapsed. Seeded
  // once per modal open (it mounts fresh).
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() =>
    defaultExpandedVersions(releases, lastSeen),
  );

  const toggle = (version: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(version)) next.delete(version);
      else next.add(version);
      return next;
    });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <ModalBackdrop onDismiss={onClose} zIndex={205}>
      <div
        {...stopBackdropEvents}
        style={{
          width: "min(720px, 92vw)",
          maxHeight: "min(640px, 86vh)",
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          backdropFilter: "blur(24px)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          boxShadow: "0 24px 80px rgba(0,0,0,0.5)",
        }}
      >
        {/* Title bar */}
        <div
          style={{
            flexShrink: 0,
            minHeight: 52,
            borderBottom: "1px solid var(--border)",
            background: "var(--bg-panel)",
            display: "flex",
            alignItems: "center",
            padding: "0 16px",
            gap: 10,
          }}
        >
          <span
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Release notes
          </span>
          <span
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            {releases.length} release{releases.length !== 1 ? "s" : ""}
          </span>
          <div style={{ flex: 1 }} />
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            style={{
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              width: 28,
              height: 28,
              border: "none",
              background: "transparent",
              color: "var(--text-secondary)",
              borderRadius: 5,
              cursor: "pointer",
              padding: 0,
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        {/* Scrollable accordion */}
        <div
          style={{
            overflowY: "auto",
            padding: "8px 20px 14px",
            color: "var(--text-secondary)",
            fontFamily: "var(--font-ui)",
          }}
        >
          {releases.map((r, idx) => {
            const isUnseen =
              lastSeen !== null && compareSemver(r.version, lastSeen) > 0;
            const rowBorder =
              idx > 0 ? "1px solid var(--border)" : "1px solid transparent";

            if (!releaseHasNotes(r)) {
              // Plumbing-only release: compact muted one-liner — no chevron,
              // nothing to expand. Left padding aligns with chevron column.
              return (
                <div
                  key={r.version}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    padding: "7px 0 7px 22px",
                    borderTop: rowBorder,
                    color: "var(--text-tertiary)",
                  }}
                >
                  <span
                    style={{
                      fontSize: 12,
                      fontWeight: 600,
                      color: "var(--text-secondary)",
                    }}
                  >
                    v{r.version}
                  </span>
                  {r.date && <span style={{ fontSize: 11 }}>{r.date}</span>}
                  <span style={{ fontSize: 11 }}>· No release notes</span>
                  {isUnseen && <NewBadge />}
                  <div style={{ flex: 1 }} />
                  {r.releaseUrl && <OpenReleaseButton url={r.releaseUrl} />}
                </div>
              );
            }

            const isOpen = expanded.has(r.version);
            return (
              <div key={r.version} style={{ borderTop: rowBorder }}>
                <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                  <button
                    type="button"
                    onClick={() => toggle(r.version)}
                    aria-expanded={isOpen}
                    style={{
                      flex: 1,
                      minWidth: 0,
                      display: "flex",
                      alignItems: "center",
                      gap: 8,
                      padding: "8px 0",
                      border: "none",
                      background: "transparent",
                      cursor: "pointer",
                      textAlign: "left",
                      fontFamily: "var(--font-ui)",
                    }}
                  >
                    <ChevronDownIcon
                      style={{
                        width: 14,
                        height: 14,
                        flexShrink: 0,
                        color: "var(--text-tertiary)",
                        transform: isOpen ? "none" : "rotate(-90deg)",
                        transition: "transform var(--t-fast)",
                      }}
                    />
                    <span
                      style={{
                        fontSize: 13,
                        fontWeight: 700,
                        color: "var(--text-primary)",
                      }}
                    >
                      v{r.version}
                    </span>
                    {r.date && (
                      <span
                        style={{ fontSize: 11, color: "var(--text-tertiary)" }}
                      >
                        {r.date}
                      </span>
                    )}
                    {isUnseen && <NewBadge />}
                  </button>
                  {r.releaseUrl && <OpenReleaseButton url={r.releaseUrl} />}
                </div>
                {isOpen && (
                  <div style={{ padding: "0 0 10px 22px" }}>
                    <ReactMarkdown
                      remarkPlugins={[remarkGfm]}
                      components={MODAL_COMPONENTS}
                      skipHtml
                    >
                      {r.body}
                    </ReactMarkdown>
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* Footer */}
        <div
          style={{
            flexShrink: 0,
            borderTop: "1px solid var(--border)",
            background: "var(--bg-panel)",
            padding: "10px 16px",
            display: "flex",
            justifyContent: "flex-end",
          }}
        >
          <button
            type="button"
            onClick={() => void openUrl(ALL_RELEASES_URL)}
            style={{
              background: "transparent",
              border: "none",
              padding: 0,
              color: "var(--accent)",
              cursor: "pointer",
              fontSize: 12,
              fontFamily: "var(--font-ui)",
            }}
          >
            View all releases on GitHub ↗
          </button>
        </div>
      </div>
    </ModalBackdrop>
  );
}
