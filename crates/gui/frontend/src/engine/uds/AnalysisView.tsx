/** The uds Results view: the engine's report blocks rendered live —
 * shared machinery in `components/analysis`.
 *
 * The drainage criteria (freeboard, capacity threshold, velocity band) are
 * edited from the project toolbar, over a store shared with it, so an edit
 * there re-judges these blocks on its own frame. The valuation travels
 * with the request rather than being re-read from disk, so an edit can
 * never race its own save.
 */

import { useAppState } from "../../AppContext";
import { BlockAnalysisView } from "../../components/analysis/BlockAnalysisView";
import { useCriteriaValuation } from "../../hooks/criteriaValuation";

export function UdsAnalysisView() {
  const { activeProjectId } = useAppState();
  const { valuation } = useCriteriaValuation(activeProjectId);
  // Undefined until the saved valuation is known: sending `{}` would be a
  // decision — every criterion at its default — and would override the
  // criteria on disk for as long as the read took.
  return <BlockAnalysisView criteria={valuation} />;
}
