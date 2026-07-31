/**
 * Per-block option editor for the report builder.
 *
 * Renders whatever the ENGINE describes (hydra-common spec §3.2.1) — this
 * component knows no block ids and no option keys, so a second engine's
 * blocks get an editor without a line of frontend work. Defaults and unit
 * labels arrive already resolved for the target's model, so nothing here
 * converts a unit or decides what a pressure is.
 *
 * An empty control means "use the engine's default": the key is removed from
 * the options object entirely rather than written as null, so production
 * applies the same default a hand-authored template would get.
 */

import { useEffect, useState } from "react";
import { ACCENT } from "../../../hooks";
import type { OptionKind, ReportOptionInfo } from "../../../hooks/reports";

/** An options object, or `undefined` when the block carries none. */
export type OptionValues = Record<string, unknown> | undefined;

/** Set `key`, or clear it when `value` is undefined. Returns undefined once
 * nothing is left, so a block that is back to defaults writes no `options`
 * member into the template at all. */
export function withOption(
  values: OptionValues,
  key: string,
  value: unknown,
): OptionValues {
  const next: Record<string, unknown> = { ...(values ?? {}) };
  if (value === undefined) delete next[key];
  else next[key] = value;
  return Object.keys(next).length > 0 ? next : undefined;
}

/** Render a number list as the comma-separated text the field shows. */
export function formatNumberList(values: readonly number[]): string {
  return values.join(", ");
}

export type ParseResult =
  | { ok: true; values: number[] }
  | { ok: false; error: string };

/**
 * Parse the comma-separated number list a user typed.
 *
 * Blank is valid and means "use the default" — the caller clears the key.
 * Ascent and minimum length are enforced here only to keep the user from
 * saving a template the engine will reject at render time; production
 * validates independently regardless.
 */
export function parseNumberList(
  text: string,
  opts: { minLen: number | null; ascending: boolean },
): ParseResult {
  const trimmed = text.trim();
  if (trimmed === "") return { ok: true, values: [] };

  const values: number[] = [];
  for (const part of trimmed.split(",")) {
    const piece = part.trim();
    if (piece === "") return { ok: false, error: "Remove the empty entry" };
    const n = Number(piece);
    if (!Number.isFinite(n))
      return { ok: false, error: `"${piece}" is not a number` };
    values.push(n);
  }
  if (opts.minLen != null && values.length < opts.minLen) {
    return {
      ok: false,
      error: `Give at least ${opts.minLen} value${opts.minLen === 1 ? "" : "s"}`,
    };
  }
  if (opts.ascending) {
    for (let i = 1; i < values.length; i++) {
      if (values[i] <= values[i - 1]) {
        return { ok: false, error: "Values must increase from left to right" };
      }
    }
  }
  return { ok: true, values };
}

/** Bounds message for a number/integer entry, or null when acceptable. */
export function numberIssue(value: number, kind: OptionKind): string | null {
  if (!Number.isFinite(value)) return "Enter a number";
  if (kind.type === "integer" && !Number.isInteger(value)) {
    return "Enter a whole number";
  }
  if (kind.type !== "number" && kind.type !== "integer") return null;
  if (kind.min != null && value < kind.min)
    return `Must be at least ${kind.min}`;
  if (kind.max != null && value > kind.max)
    return `Must be at most ${kind.max}`;
  return null;
}

/** Placeholder text for a control whose value is unset. */
function defaultHint(kind: OptionKind): string {
  switch (kind.type) {
    case "number":
    case "integer":
      return kind.default == null ? "none" : String(kind.default);
    case "text":
      return kind.default ?? "none";
    case "numberList":
      return kind.default == null ? "none" : formatNumberList(kind.default);
    case "choice":
      return kind.default ?? "none";
    default:
      return "";
  }
}

const fieldStyle: React.CSSProperties = {
  width: "100%",
  padding: "4px 6px",
  borderRadius: 4,
  border: "1px solid var(--border)",
  background: "var(--bg-base)",
  color: "var(--text-primary)",
  fontSize: "var(--text-sm)",
  fontFamily: "var(--font-ui)",
};

function Row({
  descriptor,
  children,
  error,
}: {
  descriptor: ReportOptionInfo;
  children: React.ReactNode;
  error: string | null;
}) {
  return (
    <label style={{ display: "block", marginBottom: 10 }}>
      <span
        style={{
          display: "block",
          fontSize: "var(--text-sm)",
          color: "var(--text-secondary)",
          marginBottom: 3,
        }}
      >
        {descriptor.label}
        {descriptor.unit ? (
          <span style={{ color: "var(--text-tertiary)" }}>
            {" "}
            ({descriptor.unit})
          </span>
        ) : null}
      </span>
      {children}
      <span
        style={{
          display: "block",
          fontSize: "var(--text-xs)",
          color: error ? "var(--danger, #d64545)" : "var(--text-tertiary)",
          marginTop: 3,
          lineHeight: 1.35,
        }}
      >
        {error ?? descriptor.help}
      </span>
    </label>
  );
}

/** One control, chosen by the descriptor's kind. */
function OptionControl({
  descriptor,
  value,
  onChange,
}: {
  descriptor: ReportOptionInfo;
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const { kind } = descriptor;

  // Text-shaped controls keep a draft so a half-typed "0, 10," survives.
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setDraft(null), [descriptor.key]);

  if (kind.type === "boolean") {
    return (
      <Row descriptor={descriptor} error={null}>
        <input
          type="checkbox"
          checked={
            value === undefined ? (kind.default ?? false) : value === true
          }
          onChange={(e) => onChange(e.target.checked)}
          style={{ accentColor: ACCENT }}
        />
      </Row>
    );
  }

  if (kind.type === "choice" || kind.type === "multiChoice") {
    const selected =
      kind.type === "choice"
        ? typeof value === "string"
          ? value
          : (kind.default ?? "")
        : null;
    if (kind.type === "choice") {
      return (
        <Row descriptor={descriptor} error={null}>
          <select
            value={selected ?? ""}
            onChange={(e) =>
              onChange(e.target.value === "" ? undefined : e.target.value)
            }
            style={fieldStyle}
          >
            <option value="">Default</option>
            {kind.items.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
        </Row>
      );
    }
    const chosen = new Set(
      Array.isArray(value) ? (value as string[]) : (kind.default ?? []),
    );
    return (
      <Row descriptor={descriptor} error={null}>
        <span style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          {kind.items.map((item) => (
            <label
              key={item.value}
              style={{ display: "flex", gap: 6, fontSize: "var(--text-sm)" }}
            >
              <input
                type="checkbox"
                checked={chosen.has(item.value)}
                onChange={(e) => {
                  const next = new Set(chosen);
                  if (e.target.checked) next.add(item.value);
                  else next.delete(item.value);
                  onChange(next.size === 0 ? undefined : [...next]);
                }}
                style={{ accentColor: ACCENT }}
              />
              {item.label}
            </label>
          ))}
        </span>
      </Row>
    );
  }

  if (kind.type === "numberList") {
    const shown =
      draft ??
      (Array.isArray(value) ? formatNumberList(value as number[]) : "");
    return (
      <Row descriptor={descriptor} error={error}>
        <input
          type="text"
          value={shown}
          placeholder={defaultHint(kind)}
          onChange={(e) => {
            const text = e.target.value;
            setDraft(text);
            const parsed = parseNumberList(text, {
              minLen: kind.minLen,
              ascending: kind.ascending,
            });
            if (!parsed.ok) {
              setError(parsed.error);
              return;
            }
            setError(null);
            onChange(parsed.values.length === 0 ? undefined : parsed.values);
          }}
          style={fieldStyle}
        />
      </Row>
    );
  }

  // number | integer | text
  const shown =
    draft ?? (value === undefined || value === null ? "" : String(value));
  return (
    <Row descriptor={descriptor} error={error}>
      <input
        type={kind.type === "text" ? "text" : "number"}
        value={shown}
        placeholder={defaultHint(kind)}
        onChange={(e) => {
          const text = e.target.value;
          setDraft(text);
          if (text.trim() === "") {
            setError(null);
            onChange(undefined);
            return;
          }
          if (kind.type === "text") {
            setError(null);
            onChange(text);
            return;
          }
          const n = Number(text);
          const issue = numberIssue(n, kind);
          setError(issue);
          if (!issue) onChange(n);
        }}
        style={fieldStyle}
      />
    </Row>
  );
}

/** The option form for one block. Renders nothing when it takes none. */
export function BlockOptions({
  descriptors,
  values,
  onChange,
}: {
  descriptors: ReportOptionInfo[];
  values: OptionValues;
  onChange: (next: OptionValues) => void;
}) {
  // No container of its own: the caller owns the settings panel, so options
  // and the heading field sit in one visual group rather than two.
  if (descriptors.length === 0) return null;
  return (
    <>
      {descriptors.map((descriptor) => (
        <OptionControl
          key={descriptor.key}
          descriptor={descriptor}
          value={values?.[descriptor.key]}
          onChange={(next) =>
            onChange(withOption(values, descriptor.key, next))
          }
        />
      ))}
    </>
  );
}
