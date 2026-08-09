/** The project toolbar's criteria control, one per engine.
 *
 * Criteria are project-scoped by design — per-scenario criteria would make
 * two scenarios' compliance figures incomparable, which is the point of
 * scenarios — so the toolbar, the only project-scoped chrome, is where the
 * standard belongs. From there one control reaches the canvas it recolours,
 * the results it judges, and the report it is exported into. It was two
 * unrelated triggers before: the map legend's popover and a chip on the
 * Results tab strip, neither reachable from where the third surface reads.
 *
 * The chip is the word alone, with the standard read back on hover: on a
 * toolbar a summary would have to compete with the scenario strip for
 * width and lose.
 */

import { useActiveProject } from "../../AppContext";
import { useProjectCriteria } from "../../hooks";
import { useCriteriaCatalog } from "../../hooks/criteriaCatalog";
import { useCriteriaValuation } from "../../hooks/criteriaValuation";
import { criteriaSummary } from "../../pages/project/criteriaSummary";
import { useUnitSystem } from "../../units";
import { CriteriaValuationEditor } from "../analysis/CriteriaValuationEditor";
import {
  defaultValuation,
  type Valuation,
  valuationSummary,
} from "../analysis/criteria";
import { ChipPopover } from "../ui/ChipPopover";
import { CriteriaEditor } from "./CriteriaEditor";

/** The wds control: the bespoke editor the canvas legend also opens, over
 * the typed store the canvas colours from. */
export function WdsCriteriaControl() {
  const { project } = useActiveProject();
  const { criteria, setCriteria } = useProjectCriteria(project?.id ?? null);
  const sys = useUnitSystem();
  return (
    <ChipPopover label="Criteria" summary={criteriaSummary(criteria, sys)}>
      <CriteriaEditor criteria={criteria} onChange={setCriteria} />
    </ChipPopover>
  );
}

/** Any engine publishing a criteria catalog: fields, units and defaults
 * all arrive as data, so this control never names a criterion. */
export function CatalogCriteriaControl() {
  const { project } = useActiveProject();
  const projectId = project?.id ?? null;
  const catalog = useCriteriaCatalog(projectId);
  const { valuation, setValuation } = useCriteriaValuation(projectId);
  const sys = useUnitSystem();

  // An engine that catalogs no criteria has no standard to edit.
  if (catalog.length === 0) return null;
  const values: Valuation = valuation ?? defaultValuation(catalog);
  return (
    <ChipPopover
      label="Criteria"
      summary={valuationSummary(catalog, values, sys)}
    >
      <CriteriaValuationEditor
        catalog={catalog}
        values={values}
        onChange={setValuation}
      />
    </ChipPopover>
  );
}
