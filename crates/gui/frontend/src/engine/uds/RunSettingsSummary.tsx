/** The uds implementation of the run modal's settings card: the model
 * file's settings, read-only, in the same card presentation the wds
 * summary uses. */

import { useEffect, useState } from "react";
import { SummaryRows } from "../../components/modals/RunModal/helpers";
import { getSimSummaryPairs, type SimSummaryPair } from "../../hooks";
import { fetchInto } from "../../hooks/fetchInto";
import type { RunSettingsSummaryProps } from "../registry";

export function UdsRunSettingsSummary({ projectId }: RunSettingsSummaryProps) {
  const [pairs, setPairs] = useState<SimSummaryPair[]>([]);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    setLoading(true);
    return fetchInto(getSimSummaryPairs(projectId), (p) => {
      setPairs(p);
      setLoading(false);
    });
  }, [projectId]);

  if (pairs.length > 0)
    return (
      <SummaryRows
        rows={pairs.map((p) => ({ label: p.label, value: p.value }))}
      />
    );
  return (
    <div style={{ fontSize: "var(--text-md)", color: "var(--text-tertiary)" }}>
      {loading ? "Loading…" : "Unavailable: this project has no model yet."}
    </div>
  );
}
