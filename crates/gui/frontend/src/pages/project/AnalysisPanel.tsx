/** The wds analysis page: the criteria editor above the engine's report
 * blocks, rendered live — the analysis-as-blocks convergence. The blocks
 * ARE the report: what this page shows is exactly what `generateReport`
 * puts in a document, produced by the same engine code and resolved into
 * the same display units, so the two can never disagree.
 *
 * Criteria ride along with the block request; the backend maps them onto
 * the criteria-shaped blocks' options (minimum service pressure onto the
 * compliance block, the pressure and velocity bands onto the threshold
 * charts) with the engine's own unit factors.
 */

import { useActiveProject } from "../../AppContext";
import { BlockAnalysisView } from "../../components/analysis/BlockAnalysisView";
import { CriteriaEditor } from "../../components/panels/CriteriaEditor";
import { useProjectCriteria } from "../../hooks";

export function AnalysisPanel() {
  const { project } = useActiveProject();
  const { criteria, setCriteria } = useProjectCriteria(project?.id ?? null);

  return (
    <BlockAnalysisView
      criteria={criteria}
      header={<CriteriaEditor criteria={criteria} onChange={setCriteria} />}
    />
  );
}
