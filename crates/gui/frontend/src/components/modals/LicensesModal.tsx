/**
 * About → licensing.
 *
 * Three things a shipped binary owes its user and had nowhere to say:
 * which licence it is under, what that licence asks of them, and the
 * copyright notices of the nine hundred open-source packages it is built
 * from. All three are embedded in the app, so this panel works on a
 * machine with no network and no repository.
 *
 * The components tab renders every row rather than virtualising: nine
 * hundred is a list, not a network, and the whole point of the search box
 * is that a reader arriving from an advisory can find one row — which a
 * virtualiser would then have to be asked to scroll to.
 */

import {
  ArrowTopRightOnSquareIcon,
  ChevronDownIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  getLicenseInfo,
  getThirdPartyLicenseText,
  type LicenseInfo,
  listThirdPartyComponents,
  matchesComponentQuery,
  noticeFallback,
  repoLink,
  type ThirdPartyComponent,
} from "../../hooks/licenses";
import { MarkdownBody } from "../ui/MarkdownBody";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";
import { Spinner } from "../ui/Spinner";

export type LicenseTab = "hydra" | "commercial" | "components";

const TABS: Array<{ id: LicenseTab; label: string }> = [
  { id: "hydra", label: "Hydra's licence" },
  { id: "commercial", label: "Commercial use" },
  { id: "components", label: "Open-source components" },
];

/** Licence text, as it was written: fixed width, wrapped, never reflowed
 *  into prose. A licence is a legal document and its line breaks are part
 *  of how it reads. */
const LEGAL_TEXT: React.CSSProperties = {
  margin: 0,
  fontFamily: "var(--font-mono)",
  fontSize: "var(--text-sm)",
  lineHeight: 1.55,
  color: "var(--text-secondary)",
  whiteSpace: "pre-wrap",
  wordBreak: "break-word",
};

const PANEL_SCROLL: React.CSSProperties = {
  flex: 1,
  minHeight: 0,
  overflow: "auto",
  padding: "16px 20px 20px",
};

/**
 * The header and the tab strip: chrome, and chrome does not shrink.
 *
 * The panel has no height, only a `max-height`, so on a long page the flex
 * column has to shrink something to fit. The page itself is `flex: 1`,
 * whose basis of zero means it absorbs none of that shrinking — so all of
 * it lands on these two. `min-height: auto` would floor them at their
 * content, except that floor is dropped for any item whose `overflow` is
 * not `visible`, and the tab strip scrolls its tabs on a narrow window.
 *
 * So on the two long pages — the commercial document, nine hundred
 * component rows — the strip collapsed to its own padding and a border
 * line, while the header beside it (no overflow, so still floored) stayed
 * put. What was left was a panel titled "Licences" showing a document with
 * no tabs: it read as a third modal stacked on the second, with no way
 * back to the page it had been opened on.
 *
 * The header carries the same declaration though nothing has collapsed it
 * yet — it is the same kind of box, one `overflow` away from the same bug.
 *
 * Exported so the layout test measures the styles that ship. This is
 * geometry, so no other layer can see it: jsdom reports every height as
 * zero and would call a collapsed strip and a healthy one the same.
 */
export const MODAL_HEADER: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 12,
  padding: "14px 20px 0",
  flexShrink: 0,
};

export const TAB_STRIP: React.CSSProperties = {
  display: "flex",
  gap: 4,
  padding: "6px 20px 0",
  borderBottom: "1px solid var(--border)",
  overflowX: "auto",
  flexShrink: 0,
};

/** The panel itself — a column of header, tabs and one scrolling page. */
export const MODAL_PANEL: React.CSSProperties = {
  width: "min(760px, 92vw)",
  maxHeight: "min(680px, 86vh)",
  background: "var(--bg-card)",
  border: "1px solid var(--border)",
  borderRadius: 10,
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
  boxShadow: "0 24px 80px rgba(0,0,0,0.5)",
};

function TabButton({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        padding: "8px 12px",
        background: "transparent",
        border: "none",
        borderBottom: `2px solid ${active ? "var(--accent)" : "transparent"}`,
        color: active ? "var(--text-primary)" : "var(--text-secondary)",
        fontWeight: active ? 600 : 400,
        fontSize: "var(--text-lg)",
        fontFamily: "var(--font-ui)",
        cursor: "pointer",
        whiteSpace: "nowrap",
      }}
    >
      {label}
    </button>
  );
}

function LinkButton({ label, url }: { label: string; url: string }) {
  return (
    <button
      type="button"
      onClick={() => void openUrl(url)}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 5,
        padding: "5px 10px",
        border: "1px solid var(--border-hover)",
        borderRadius: 6,
        background: "transparent",
        color: "var(--text-primary)",
        fontSize: "var(--text-md)",
        fontFamily: "var(--font-ui)",
        cursor: "pointer",
      }}
    >
      {label}
      <ArrowTopRightOnSquareIcon style={{ width: 12, height: 12 }} />
    </button>
  );
}

/** Hydra's own licence: what it means first, then the text itself. Nobody
 *  reads 663 lines of legalese to answer "can I use this at work", so the
 *  answer comes first and the text is one click away. */
function HydraTab({ info }: { info: LicenseInfo }) {
  const [showText, setShowText] = useState(false);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      <div>
        <div
          style={{
            fontSize: "var(--text-xl)",
            fontWeight: 600,
            marginBottom: 6,
          }}
        >
          Hydra is free software
        </div>
        <p
          style={{
            margin: "0 0 8px",
            fontSize: "var(--text-lg)",
            lineHeight: 1.6,
            color: "var(--text-secondary)",
          }}
        >
          Hydra is published under the GNU Affero General Public License v3.0 (
          {info.spdx}). You may run it for any purpose, including commercial
          work, and the models, results and reports it produces are yours. The
          licence covers the software, not its output.
        </p>
        <p
          style={{
            margin: 0,
            fontSize: "var(--text-lg)",
            lineHeight: 1.6,
            color: "var(--text-secondary)",
          }}
        >
          It asks something of you only if you distribute Hydra's code inside
          your own product, or run a modified Hydra as a network service. Then
          that work has to be released under the same licence.
        </p>
      </div>

      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <LinkButton label="Source code" url={info.sourceUrl} />
        <button
          type="button"
          onClick={() => setShowText((v) => !v)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 5,
            padding: "5px 10px",
            border: "1px solid var(--border-hover)",
            borderRadius: 6,
            background: "transparent",
            color: "var(--text-primary)",
            fontSize: "var(--text-md)",
            fontFamily: "var(--font-ui)",
            cursor: "pointer",
          }}
        >
          {showText ? "Hide" : "Read"} the full licence
          <ChevronDownIcon
            style={{
              width: 12,
              height: 12,
              transform: showText ? "rotate(180deg)" : "none",
              transition: "transform var(--t-fast)",
            }}
          />
        </button>
      </div>

      {showText && (
        <div
          style={{
            border: "1px solid var(--border)",
            borderRadius: 8,
            background: "var(--bg-input, var(--bg-elevated))",
            padding: 14,
          }}
        >
          <pre style={LEGAL_TEXT}>{info.text}</pre>
        </div>
      )}
    </div>
  );
}

/** One component row: what it is, and its notices under it. */
function ComponentRow({
  component,
  textFor,
  onRequestText,
}: {
  component: ThirdPartyComponent;
  textFor: (index: number) => string | undefined;
  onRequestText: (index: number) => void;
}) {
  const [open, setOpen] = useState<number | null>(null);
  const fallback = noticeFallback(component);

  return (
    <div style={{ padding: "9px 0", borderBottom: "1px solid var(--border)" }}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <span style={{ fontSize: "var(--text-lg)", fontWeight: 500 }}>
          {component.name}
        </span>
        <span
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            fontVariantNumeric: "tabular-nums",
          }}
        >
          {component.version}
        </span>
        <span
          style={{
            fontSize: "var(--text-2xs)",
            fontWeight: 700,
            letterSpacing: "0.06em",
            textTransform: "uppercase",
            color: "var(--text-tertiary)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            padding: "0 4px",
          }}
        >
          {component.ecosystem}
        </span>
        <span
          style={{
            marginLeft: "auto",
            fontSize: "var(--text-sm)",
            color: "var(--text-secondary)",
          }}
        >
          {component.spdx || "licence not declared"}
        </span>
      </div>

      <div
        style={{
          display: "flex",
          gap: 6,
          flexWrap: "wrap",
          marginTop: 5,
          alignItems: "center",
        }}
      >
        {component.files.map((file) => {
          const isOpen = open === file.text;
          return (
            <button
              key={`${file.name}:${file.text}`}
              type="button"
              onClick={() => {
                setOpen(isOpen ? null : file.text);
                if (!isOpen) onRequestText(file.text);
              }}
              style={{
                padding: "2px 8px",
                border: "1px solid var(--border)",
                borderRadius: 5,
                background: isOpen ? "var(--accent-dim)" : "transparent",
                color: isOpen ? "var(--accent)" : "var(--text-secondary)",
                fontSize: "var(--text-sm)",
                fontFamily: "var(--font-ui)",
                cursor: "pointer",
              }}
            >
              {file.name}
            </button>
          );
        })}
        {fallback && (
          <span
            style={{
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
              lineHeight: 1.5,
            }}
          >
            {fallback}
          </span>
        )}
        {component.url && (
          <button
            type="button"
            onClick={() => void openUrl(component.url)}
            aria-label={`Open ${component.name}'s home page`}
            style={{
              display: "inline-flex",
              alignItems: "center",
              border: "none",
              background: "transparent",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              padding: 2,
            }}
          >
            <ArrowTopRightOnSquareIcon style={{ width: 12, height: 12 }} />
          </button>
        )}
      </div>

      {open !== null && (
        <div
          style={{
            marginTop: 8,
            border: "1px solid var(--border)",
            borderRadius: 8,
            background: "var(--bg-input, var(--bg-elevated))",
            padding: 12,
            maxHeight: 300,
            overflow: "auto",
          }}
        >
          {textFor(open) === undefined ? (
            <div
              style={{
                display: "flex",
                gap: 8,
                alignItems: "center",
                color: "var(--text-tertiary)",
                fontSize: "var(--text-md)",
              }}
            >
              <Spinner />
              Loading…
            </div>
          ) : (
            <pre style={LEGAL_TEXT}>{textFor(open)}</pre>
          )}
        </div>
      )}
    </div>
  );
}

function ComponentsTab({
  components,
  loading,
}: {
  components: ThirdPartyComponent[];
  loading: boolean;
}) {
  const [query, setQuery] = useState("");
  // Texts are fetched one at a time and kept: a reader comparing two
  // packages should not pay for the same Apache-2.0 text twice.
  const [texts, setTexts] = useState<ReadonlyMap<number, string>>(new Map());

  const requestText = useCallback(
    (index: number) => {
      if (texts.has(index)) return;
      void getThirdPartyLicenseText(index).then((text) => {
        if (text === null) return;
        setTexts((prev) => new Map(prev).set(index, text));
      });
    },
    [texts],
  );
  const textFor = useCallback((index: number) => texts.get(index), [texts]);

  const shown = useMemo(
    () => components.filter((c) => matchesComponentQuery(c, query)),
    [components, query],
  );
  const rust = components.filter((c) => c.ecosystem === "rust").length;
  const npm = components.length - rust;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <p
        style={{
          margin: 0,
          fontSize: "var(--text-md)",
          color: "var(--text-secondary)",
          lineHeight: 1.6,
        }}
      >
        Hydra is built on {rust} Rust crates and {npm} npm packages. Their
        licences and copyright notices are reproduced below, as those licences
        ask.
      </p>
      <input
        type="search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search by name, version or licence"
        style={{
          height: 30,
          background: "var(--bg-input, var(--bg-card))",
          border: "1px solid var(--border)",
          borderRadius: 6,
          padding: "0 10px",
          color: "var(--text-primary)",
          fontFamily: "var(--font-ui)",
          fontSize: "var(--text-lg)",
          outline: "none",
        }}
      />
      {loading && (
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "center",
            color: "var(--text-tertiary)",
            fontSize: "var(--text-lg)",
            padding: "16px 0",
          }}
        >
          <Spinner />
          Loading…
        </div>
      )}
      {!loading && shown.length === 0 && (
        <div
          style={{
            color: "var(--text-tertiary)",
            fontSize: "var(--text-lg)",
            padding: "16px 0",
          }}
        >
          Nothing matches “{query}”.
        </div>
      )}
      {shown.map((c) => (
        <ComponentRow
          key={`${c.ecosystem}:${c.name}:${c.version}`}
          component={c}
          textFor={textFor}
          onRequestText={requestText}
        />
      ))}
    </div>
  );
}

export function LicensesModal({
  tab: initialTab,
  onClose,
}: {
  tab: LicenseTab;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<LicenseTab>(initialTab);
  const [info, setInfo] = useState<LicenseInfo | null>(null);
  const [components, setComponents] = useState<ThirdPartyComponent[] | null>(
    null,
  );

  useEffect(() => {
    void getLicenseInfo().then(setInfo);
    void listThirdPartyComponents().then(setComponents);
  }, []);

  // Escape belongs to the topmost overlay, and this one is usually opened
  // from inside the settings drawer — which has its own window-level
  // Escape handler, registered first. Left to run, that handler closed the
  // drawer *and* this panel with it, so a key that should have stepped
  // back one level dismissed both. Capture phase runs before any bubble
  // listener whatever the registration order, and stopping the event there
  // is what makes "topmost wins" true rather than "whoever mounted first".
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopImmediatePropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const resolveLink = useCallback(
    (href: string) => repoLink(href, info?.sourceUrl ?? ""),
    [info?.sourceUrl],
  );

  return (
    <ModalBackdrop onDismiss={onClose} zIndex={210}>
      <div
        {...stopBackdropEvents}
        role="dialog"
        aria-label="Licences"
        style={MODAL_PANEL}
      >
        <div style={MODAL_HEADER}>
          <h2
            style={{
              margin: 0,
              fontSize: "var(--text-2xl)",
              fontWeight: 700,
            }}
          >
            Licences
          </h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close licences"
            style={{
              width: 26,
              height: 26,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              borderRadius: 6,
              border: "1px solid var(--border)",
              background: "transparent",
              color: "var(--text-secondary)",
              cursor: "pointer",
              flexShrink: 0,
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        <div style={TAB_STRIP}>
          {TABS.map((t) => (
            <TabButton
              key={t.id}
              active={tab === t.id}
              label={t.label}
              onClick={() => setTab(t.id)}
            />
          ))}
        </div>

        <div style={PANEL_SCROLL}>
          {tab === "hydra" &&
            (info ? (
              <HydraTab info={info} />
            ) : (
              <div
                style={{
                  display: "flex",
                  gap: 8,
                  alignItems: "center",
                  color: "var(--text-tertiary)",
                  fontSize: "var(--text-lg)",
                }}
              >
                <Spinner />
                Loading…
              </div>
            ))}
          {tab === "commercial" && info && (
            <MarkdownBody resolveLink={resolveLink}>
              {info.commercial}
            </MarkdownBody>
          )}
          {tab === "components" && (
            <ComponentsTab
              components={components ?? []}
              loading={components === null}
            />
          )}
        </div>
      </div>
    </ModalBackdrop>
  );
}
