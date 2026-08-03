import { PropRow } from "../../components/panels/ElementInspector/primitives";
import { SectionLabel } from "../../components/ui/SectionLabel";
import type { GenericElementValue } from "../registry";

/** Value cell: engine-authored unit label, em dash for unreported. */
function formatValue(v: GenericElementValue): string {
  if (v.value == null || !Number.isFinite(v.value)) return "—";
  const a = Math.abs(v.value);
  const text =
    a >= 1000
      ? Math.round(v.value).toLocaleString()
      : a >= 10
        ? v.value.toFixed(1)
        : v.value.toFixed(2);
  return v.unit ? `${text} ${v.unit}` : text;
}

/**
 * "Results" section of the drainage inspector bodies: the selected
 * element's current-period value for every catalog variable, in the same
 * property-table presentation the wds body uses. Shows a quiet empty state
 * before a run.
 */
export function GenericResultsTable({
  results,
}: {
  results?: GenericElementValue[] | null;
}) {
  return (
    <>
      <SectionLabel>Results</SectionLabel>
      {results && results.length > 0 ? (
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            marginBottom: 14,
          }}
        >
          <tbody>
            {results.map((v) => (
              <PropRow key={v.id} label={v.label} value={formatValue(v)} />
            ))}
          </tbody>
        </table>
      ) : (
        <div
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            marginBottom: 14,
          }}
        >
          Run a simulation to see results.
        </div>
      )}
    </>
  );
}
