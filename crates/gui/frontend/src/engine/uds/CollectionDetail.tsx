/**
 * The contents of one container element.
 *
 * A collection row can only report what a thing *is* and how big — a curve
 * with 14 points, a rule with 3 clauses — because the contents do not fit
 * a table cell. This is where they are actually shown.
 *
 * Two renderings, chosen by what the engine returned rather than by the
 * kind asked for: a table of numbers under engine-named headings, or
 * verbatim lines for containers whose content is language.
 */

import {
  type CollectionDetail as Detail,
  formatGenericValue,
  genericUnitLabel,
} from "../../hooks";
import { useUnitSystem } from "../../units";

export function CollectionDetail({
  detail,
  elementId,
}: {
  detail: Detail;
  /** The container being shown, for the panel's heading. */
  elementId: string;
}) {
  const sys = useUnitSystem();
  const hasTable = detail.rows.length > 0;
  const hasLines = detail.lines.length > 0;

  return (
    <div
      style={{
        borderTop: "1px solid var(--border)",
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
        maxHeight: "45%",
        background: "var(--bg-panel)",
      }}
    >
      <div
        style={{
          padding: "6px 12px",
          fontSize: "var(--text-sm)",
          fontWeight: 600,
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
          flexShrink: 0,
        }}
      >
        {elementId}
      </div>

      <div style={{ overflow: "auto", flex: 1, minHeight: 0 }}>
        {hasTable && (
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontFamily: "var(--font-mono)",
              fontSize: "var(--text-md)",
            }}
          >
            <thead>
              <tr>
                {detail.columns.map((c, i) => {
                  const unit = genericUnitLabel(
                    detail.quantities[i] ?? undefined,
                    sys,
                  );
                  return (
                    <th
                      key={c}
                      style={{
                        textAlign: "left",
                        padding: "4px 12px",
                        color: "var(--text-tertiary)",
                        fontWeight: 500,
                        fontFamily: "var(--font-ui)",
                        borderBottom: "1px solid var(--border)",
                        position: "sticky",
                        top: 0,
                        background: "var(--bg-panel)",
                      }}
                    >
                      {unit ? `${c} (${unit})` : c}
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {detail.rows.map((row) => (
                <tr key={row.join(",")}>
                  {row.map((v, i) => (
                    <td
                      key={detail.columns[i] ?? i}
                      style={{
                        padding: "3px 12px",
                        borderBottom: "1px solid rgba(255,255,255,0.04)",
                      }}
                    >
                      {/* Values arrive SI; the column's quantity is what
                          makes them displayable in the user's system. */}
                      {formatGenericValue(
                        v,
                        detail.quantities[i] ?? undefined,
                        sys,
                        false,
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {hasLines && (
          <pre
            style={{
              margin: 0,
              padding: "6px 12px",
              fontFamily: "var(--font-mono)",
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
              whiteSpace: "pre-wrap",
            }}
          >
            {detail.lines.join("\n")}
          </pre>
        )}

        {/* An external time series' contents live in a file the engine
            never reads, so "nothing to show" is an answer, not a failure. */}
        {!hasTable && !hasLines && (
          <div
            style={{
              padding: "10px 12px",
              fontSize: "var(--text-md)",
              color: "var(--text-tertiary)",
            }}
          >
            Nothing to show — this entry's contents are held outside the model
            file.
          </div>
        )}
      </div>
    </div>
  );
}
