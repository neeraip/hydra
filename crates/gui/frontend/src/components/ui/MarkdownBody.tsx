/**
 * A Markdown document, rendered in the app's own typography.
 *
 * The component map used to live inside the release-notes modal, which was
 * fine while release notes were the only Markdown the app displayed. They
 * are not: the commercial-licence document is the repository's own
 * Markdown, shown in the About panel, and two copies of this map would
 * drift into two ideas of what a heading looks like.
 *
 * Raw HTML stays disabled — react-markdown's default, and the reason a
 * document fetched from GitHub or embedded from the tree can be rendered
 * at all.
 */

import { openUrl } from "@tauri-apps/plugin-opener";
import { useMemo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

function headingStyle(fontSize: number): React.CSSProperties {
  return {
    fontSize,
    fontWeight: 700,
    color: "var(--text-primary)",
    margin: "10px 0 6px",
    lineHeight: 1.4,
  };
}

/** Links open in the system browser: this window is the app, not a tab to
 *  navigate away from. */
function linkComponents(resolve: (href: string) => string): Components {
  return {
    a: ({ href, children }) => (
      <button
        type="button"
        onClick={() => {
          if (!href) return;
          const target = resolve(href);
          // Reported rather than swallowed. `openUrl` rejects when the
          // target's scheme is outside the app's opener scope, and a
          // rejection nobody handles is a link that does nothing at all —
          // which is how a `mailto:` link sat broken with no sign of it.
          openUrl(target).catch((err) => {
            console.error(`Could not open ${target}:`, err);
          });
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
      <p
        style={{
          margin: "0 0 8px",
          fontSize: "var(--text-md)",
          lineHeight: 1.6,
        }}
      >
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
      <li
        style={{ fontSize: "var(--text-md)", lineHeight: 1.6, marginBottom: 2 }}
      >
        {children}
      </li>
    ),
    code: ({ children }) => (
      <code
        style={{
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-sm)",
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
          fontSize: "var(--text-sm)",
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
      <div
        style={{ height: 1, background: "var(--border)", margin: "10px 0" }}
      />
    ),
  };
}

const IDENTITY = (href: string) => href;

export function MarkdownBody({
  children,
  resolveLink = IDENTITY,
}: {
  children: string;
  /** Rewrites a link target before it is opened — used by documents
   *  embedded from the repository, whose relative links have no sibling
   *  file to resolve against once they are inside an installed app. */
  resolveLink?: (href: string) => string;
}) {
  const components = useMemo(() => linkComponents(resolveLink), [resolveLink]);
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={components} skipHtml>
      {children}
    </ReactMarkdown>
  );
}
