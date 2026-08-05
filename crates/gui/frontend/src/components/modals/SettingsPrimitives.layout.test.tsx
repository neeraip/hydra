import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll, widthOf } from "../../layoutTest";
import { SettingRow } from "./SettingsPrimitives";

afterEach(unmountAll);

/**
 * A settings row's label column, measured in a real browser.
 *
 * The row puts its label and control at either end, so before the control
 * had a lane of its own the label column was whatever the control left
 * over. A wide control narrowed its own label and re-wrapped its
 * description while its neighbours stayed put — rows that should have read
 * as a column did not, and swapping a loading placeholder for a real
 * control moved the text beside it.
 *
 * The real `SettingRow` is rendered rather than a copy of its styles: what
 * is under test is that component's arrangement.
 */
function Row({ control }: { control: React.ReactNode }) {
  return (
    <div data-wrap style={{ width: 680 }}>
      <SettingRow
        label="Reopen last project on launch"
        description="Start Hydra straight back in the project you last had open."
      >
        {control}
      </SettingRow>
    </div>
  );
}

async function labelWidth(control: React.ReactNode): Promise<number> {
  // `[data-wrap]` is the test's own element, so the path to the label
  // column is stated from a node this file controls: wrapper → the row
  // `SettingRow` renders → its first child, the label.
  const host = await mount(<Row control={control} />);
  return widthOf(host, "[data-wrap] > div > div:first-child");
}

describe("a settings row", () => {
  /**
   * Narrow control, wide control, same label column — which is what makes
   * the rows read as a column and what keeps a loading state's placeholder
   * from moving the text when the real control replaces it.
   */
  it("gives every row the same label column, whatever its control", async () => {
    const toggle = await labelWidth(<span style={{ width: 34 }} />);
    const buttons = await labelWidth(<span style={{ width: 210 }} />);
    expect(toggle).toBe(buttons);
  });

  /**
   * And the lane is genuinely reserved, not merely consistent: a row whose
   * control is nothing at all still leaves the space, so a control
   * appearing later cannot push the label.
   */
  it("reserves the lane even when there is no control", async () => {
    const empty = await labelWidth(null);
    const toggle = await labelWidth(<span style={{ width: 34 }} />);
    expect(empty).toBe(toggle);
  });

  /**
   * A control wider than the lane is allowed to take the room it needs —
   * the lane is a minimum, not a clamp. Clamping would crush a control at
   * the larger text scales instead of letting the label give way.
   */
  it("lets an oversized control take more, rather than clipping it", async () => {
    const normal = await labelWidth(<span style={{ width: 210 }} />);
    const oversized = await labelWidth(<span style={{ width: 400 }} />);
    expect(oversized).toBeLessThan(normal);
  });
});
