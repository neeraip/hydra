import { describe, expect, it } from "vitest";
import {
  DRAFT_CONTAINERS,
  type DraftContainerSizes,
  draftDirty,
} from "./draftDirty";

// Built from the declared container list rather than a literal, so a new
// container is covered by these tests the moment it is declared.
const none = Object.fromEntries(
  DRAFT_CONTAINERS.map((k) => [k, 0]),
) as unknown as DraftContainerSizes;

describe("draftDirty", () => {
  it("reports nothing staged as nothing dirty", () => {
    const { total, bySection } = draftDirty(none);
    expect(total).toBe(0);
    expect(Object.values(bySection).every((n) => n === 0)).toBe(true);
  });

  // The one that matters. "Is there unsaved work" gates the navigation
  // guard, so a container missing from the sum does not show a wrong
  // number — it lets staged work be discarded without anyone being asked.
  // Every declared container must move the total on its own.
  it("counts every declared container toward the total", () => {
    for (const key of DRAFT_CONTAINERS) {
      const { total } = draftDirty({ ...none, [key]: 1 });
      expect(total, `${key} does not reach the total`).toBe(1);
    }
  });

  it("counts every declared container toward exactly one section", () => {
    for (const key of DRAFT_CONTAINERS) {
      const { bySection } = draftDirty({ ...none, [key]: 1 });
      const touched = Object.values(bySection).filter((n) => n > 0);
      expect(
        touched,
        `${key} lands in ${touched.length} sections`,
      ).toHaveLength(1);
    }
  });

  // The total used to be summed separately from the sections, so the two
  // could disagree — a rail entry marked dirty while the guard let you
  // walk away from it.
  it("keeps the total equal to the sum of its sections", () => {
    const sizes: DraftContainerSizes = {
      ...none,
      curveEdits: 2,
      ruleDeletes: 5,
    };
    const { total, bySection } = draftDirty(sizes);
    expect(total).toBe(Object.values(bySection).reduce((a, b) => a + b, 0));
    expect(total).toBe(7);
  });

  it("groups rules with controls, as the editor does", () => {
    const { bySection } = draftDirty({ ...none, ruleAdds: 2 });
    expect(bySection.controls).toBe(2);
  });

  it("has no section for elements", () => {
    // They are not staged any more: an element edit is written and saved
    // before the field gives focus back, so there is never a count of
    // them to report and never a section for it in the rail.
    expect(Object.keys(draftDirty(none).bySection)).toEqual([
      "curves",
      "patterns",
      "controls",
    ]);
  });
});
