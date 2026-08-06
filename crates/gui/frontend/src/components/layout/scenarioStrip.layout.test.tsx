import { afterEach, describe, expect, it } from "vitest";
import type { ScenarioDto } from "../../hooks";
import { mount, unmountAll, widthOf } from "../../layoutTest";
import { ScenarioChip } from "./ProjectToolbar";

afterEach(unmountAll);

/**
 * A chip's width, measured in a real browser.
 *
 * Selecting a chip used to embolden it, and bold text is wider. So the
 * thing you clicked grew and shoved every chip to its right along — in a
 * strip whose whole purpose is choosing between them, which is the worst
 * possible place for it.
 *
 * jsdom reports every width as zero, so this is only visible here.
 */

const scenario = {
  id: "s1",
  name: "Fire flow at node 27",
  state: "simulated",
  parentId: null,
} as unknown as ScenarioDto;

async function chipWidth(isActive: boolean) {
  const host = await mount(
    <div data-wrap style={{ width: 600 }}>
      <ScenarioChip
        scenario={scenario}
        isActive={isActive}
        isStale={false}
        siblingCount={0}
        pickerOpen={false}
        onClick={() => {}}
        onTogglePicker={() => {}}
      />
    </div>,
  );
  return widthOf(host, "[data-wrap] > *");
}

describe("a scenario chip", () => {
  /** The reported defect, stated directly. */
  it("is the same width selected or not", async () => {
    expect(await chipWidth(true)).toBe(await chipWidth(false));
  });

  /** And it is not zero-width, or the assertion above passes vacuously. */
  it("has a width to compare", async () => {
    expect(await chipWidth(false)).toBeGreaterThan(40);
  });
});
