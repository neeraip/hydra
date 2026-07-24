import { useEffect, useState } from "react";
import { useAppState } from "../../../AppContext";
import { getNetworkTitle, updateNetworkTitle } from "../../../hooks/network";
import { useNetworkVersion } from "../../../hooks/NetworkVersionContext";
import {
  type ModelTitleParts,
  partsToTitleLines,
  titleLinesToParts,
} from "./modelTitle";

/**
 * Editable INP `[TITLE]` block: one title line plus up to two description
 * lines, stored in the model itself (unlike the project name, which lives in
 * Hydra's meta.json and never travels with exports).
 */
export function ModelTitleCard() {
  const { showToast } = useAppState();
  const { version } = useNetworkVersion();
  const [committed, setCommitted] = useState<ModelTitleParts | null>(null);
  const [draft, setDraft] = useState<ModelTitleParts>({
    title: "",
    description: "",
  });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const lines = await getNetworkTitle();
      if (cancelled) return;
      const parts = titleLinesToParts(lines);
      setCommitted(parts);
      setDraft(parts);
    })();
    return () => {
      cancelled = true;
    };
  }, [version]);

  // Not rendered until the first fetch resolves (matches sibling cards that
  // gate on loaded data).
  if (committed === null) return null;

  const dirty =
    draft.title !== committed.title ||
    draft.description !== committed.description;

  async function save() {
    try {
      const lines = partsToTitleLines(draft);
      await updateNetworkTitle(lines);
      const parts = titleLinesToParts(lines);
      setCommitted(parts);
      setDraft(parts);
      showToast("Model title saved", "success");
    } catch (err) {
      showToast(`Could not save title: ${String(err)}`, "error");
    }
  }

  const inputStyle: React.CSSProperties = {
    width: "100%",
    boxSizing: "border-box",
    background: "var(--bg-input, rgba(255,255,255,0.05))",
    border: "1px solid var(--border)",
    borderRadius: 6,
    color: "var(--text-primary)",
    fontSize: 12,
    fontFamily: "var(--font-ui)",
    padding: "6px 8px",
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <input
        value={draft.title}
        onChange={(e) => setDraft({ ...draft, title: e.target.value })}
        placeholder="Model title"
        aria-label="Model title"
        style={inputStyle}
      />
      <textarea
        value={draft.description}
        onChange={(e) => {
          // Cap at two description lines (EPANET [TITLE] is three lines total).
          const lines = e.target.value.split("\n");
          const next =
            lines.length <= 2
              ? e.target.value
              : `${lines[0]}\n${lines.slice(1).join(" ")}`;
          setDraft({ ...draft, description: next });
        }}
        placeholder="Description (up to two lines)"
        aria-label="Model description"
        rows={2}
        style={{ ...inputStyle, resize: "none", fontFamily: "var(--font-ui)" }}
      />
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          fontSize: 10,
          color: "var(--text-tertiary)",
        }}
      >
        <span style={{ flex: 1 }}>
          Stored in the model's INP [TITLE] — travels with exports, unlike the
          project name.
        </span>
        {dirty && (
          <>
            <button
              type="button"
              onClick={() => setDraft(committed)}
              style={{
                background: "transparent",
                border: "1px solid var(--border)",
                borderRadius: 4,
                color: "var(--text-secondary)",
                fontSize: 11,
                padding: "3px 10px",
                cursor: "pointer",
              }}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => void save()}
              style={{
                background: "var(--accent)",
                border: "none",
                borderRadius: 4,
                color: "#fff",
                fontSize: 11,
                padding: "4px 12px",
                cursor: "pointer",
              }}
            >
              Save
            </button>
          </>
        )}
      </div>
    </div>
  );
}
