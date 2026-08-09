/** The wds analysis page: the engine's report blocks rendered live — the
 * analysis-as-blocks convergence. The blocks ARE the report: what this
 * page shows is exactly what `generateReport` puts in a document, produced
 * by the same engine code, judged against the same criteria, and resolved
 * into the same display units, so the two can never disagree.
 *
 * Criteria are edited from the project toolbar (they are project-scoped,
 * and the canvas and the report read them too). They ride along with the
 * block request rather than being re-read from disk, so an edit can never
 * race its own save; the backend maps them onto the criteria-shaped
 * blocks' options with the engine's own unit factors.
 */

import { useActiveProject } from "../../AppContext";
import { BlockAnalysisView } from "../../components/analysis/BlockAnalysisView";
import { useProjectCriteria } from "../../hooks";
import { wdsValuation } from "./criteriaValuation";

export function AnalysisPanel() {
  const { project } = useActiveProject();
  const { criteria } = useProjectCriteria(project?.id ?? null);

  // The backend consumes contract valuations (hydra-common §7.3); the
  // saved wds shape bridges here, mirrored by the backend's own fallback
  // bridge for other callers.
  return <BlockAnalysisView criteria={wdsValuation(criteria)} />;
}
