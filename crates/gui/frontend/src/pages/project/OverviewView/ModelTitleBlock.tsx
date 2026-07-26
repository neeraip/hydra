import { CheckIcon, XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";
import { useAppState } from "../../../AppContext";
import { useNetworkVersion } from "../../../hooks/NetworkVersionContext";
import { getNetworkTitle, updateNetworkTitle } from "../../../hooks/network";
import {
  TITLE_DISPLAY_LINES,
  textToTitleLines,
  titleLinesToText,
} from "./modelTitle";

/**
 * The model's INP `[TITLE]` as dim inline text under the header pills.
 * Click anywhere on the text (whole block highlights on hover) to edit in a
 * free-height textarea (no line cap — EPANET's
 * three lines is convention, so display clamps at three with "View more").
 */
export function ModelTitleBlock() {
  const { showToast } = useAppState();
  const { version } = useNetworkVersion();
  const [lines, setLines] = useState<string[] | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [expanded, setExpanded] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` is an intentional retrigger — refetch the title after the network changes (import, edit, undo).
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const t = await getNetworkTitle();
      if (!cancelled) setLines(t);
    })();
    return () => {
      cancelled = true;
    };
  }, [version]);

  useEffect(() => {
    if (editing) textareaRef.current?.focus();
  }, [editing]);

  if (lines === null) return null;

  function beginEdit() {
    setDraft(titleLinesToText(lines ?? []));
    setEditing(true);
  }

  async function save() {
    try {
      const next = textToTitleLines(draft);
      await updateNetworkTitle(next);
      setLines(next);
      setEditing(false);
      showToast("Model description saved", "success");
    } catch (err) {
      showToast(`Could not save description: ${String(err)}`, "error");
    }
  }

  const iconBtn: React.CSSProperties = {
    background: "transparent",
    border: "none",
    padding: 2,
    cursor: "pointer",
    color: "var(--text-tertiary)",
    display: "inline-flex",
    alignItems: "center",
  };

  if (editing) {
    return (
      <div style={{ marginTop: 8, display: "flex", gap: 6 }}>
        <textarea
          ref={textareaRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setEditing(false);
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void save();
          }}
          placeholder="Model description (stored in the INP [TITLE])"
          rows={Math.max(2, Math.min(8, draft.split("\n").length + 1))}
          style={{
            flex: 1,
            background: "var(--bg-input, rgba(255,255,255,0.05))",
            border: "1px solid var(--border)",
            borderRadius: 6,
            color: "var(--text-secondary)",
            fontSize: 12,
            fontFamily: "var(--font-ui)",
            lineHeight: 1.5,
            padding: "6px 8px",
            resize: "vertical",
          }}
        />
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <button
            type="button"
            onClick={() => void save()}
            data-tooltip="Save (⌘↵)"
            style={{ ...iconBtn, color: "var(--accent)" }}
          >
            <CheckIcon style={{ width: 15, height: 15 }} />
          </button>
          <button
            type="button"
            onClick={() => setEditing(false)}
            data-tooltip="Cancel (Esc)"
            style={iconBtn}
          >
            <XMarkIcon style={{ width: 15, height: 15 }} />
          </button>
        </div>
      </div>
    );
  }

  const hasMore = lines.length > TITLE_DISPLAY_LINES;
  const shown = expanded ? lines : lines.slice(0, TITLE_DISPLAY_LINES);

  return (
    <div style={{ marginTop: 8 }}>
      <button
        type="button"
        onClick={beginEdit}
        className="model-title-edit-target"
        data-tooltip="Click to edit (stored in the INP [TITLE])"
        style={{
          display: "block",
          width: "100%",
          textAlign: "left",
          background: "transparent",
          border: "none",
          borderRadius: 6,
          padding: "4px 6px",
          margin: "0 -6px",
          cursor: "text",
          fontSize: 12,
          color: "var(--text-tertiary)",
          lineHeight: 1.5,
          fontFamily: "var(--font-ui)",
          whiteSpace: "pre-wrap",
          overflowWrap: "anywhere",
          fontStyle: lines.length === 0 ? "italic" : undefined,
        }}
      >
        {lines.length === 0 ? "Add a model description…" : shown.join("\n")}
      </button>
      {hasMore && (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          style={{
            ...iconBtn,
            fontSize: 11,
            color: "var(--accent)",
            padding: 0,
            marginTop: 2,
          }}
        >
          {expanded ? "View less" : "View more"}
        </button>
      )}
    </div>
  );
}
