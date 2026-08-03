/** The wds implementation of the run modal's settings card: the editable
 * [TIMES]/[OPTIONS] summary grid. */

import { useEffect, useState } from "react";
import { SummaryGrid } from "../../components/modals/RunModal/helpers";
import { getSimParams, type SimParams } from "../../hooks";
import type { RunSettingsSummaryProps } from "../registry";

export function WdsRunSettingsSummary({ projectId }: RunSettingsSummaryProps) {
  const [params, setParams] = useState<SimParams | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void getSimParams(projectId).then((p) => {
      if (cancelled) return;
      setParams(p);
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  if (params) return <SummaryGrid params={params} />;
  return (
    <div style={{ fontSize: "var(--text-md)", color: "var(--text-tertiary)" }}>
      {loading ? "Loading…" : "Unavailable — this project has no network yet."}
    </div>
  );
}
