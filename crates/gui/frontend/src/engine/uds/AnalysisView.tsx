/** The uds Results view: the engine's report blocks rendered live —
 * the analysis-as-blocks convergence, shared machinery in
 * `components/analysis`. No engine-specific header yet; drainage
 * criteria would slot in exactly where the wds editor does. */

import { BlockAnalysisView } from "../../components/analysis/BlockAnalysisView";

export function UdsAnalysisView() {
  return <BlockAnalysisView />;
}
