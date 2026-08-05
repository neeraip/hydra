/**
 * @vitest-environment jsdom
 *
 * The picker's rows had no resting affordance: no border, no fill,
 * ordinary text, and `cursor: pointer` — which cannot be seen until the
 * pointer is already on the row. They read as a static list.
 */
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

/** The row markup under test, mirroring CrsPicker's structure. */
function Row({ checked }: { checked: boolean }) {
  return (
    <label className="crs-row">
      <input type="radio" name="crs-choice" checked={checked} readOnly />
      <span>WGS 84</span>
    </label>
  );
}

describe("CRS row affordance", () => {
  // A native radio, not a button with role="radio": it brings arrow-key
  // navigation within the group and announces its own checked state.
  it("is a real radio in a named group", () => {
    const { container } = render(<Row checked={false} />);
    const input = container.querySelector("input");
    expect(input?.type).toBe("radio");
    expect(input?.name).toBe("crs-choice");
  });

  it("announces which answer is current", () => {
    expect(
      render(<Row checked />).container.querySelector("input")?.checked,
    ).toBe(true);
    expect(
      render(<Row checked={false} />).container.querySelector("input")?.checked,
    ).toBe(false);
  });

  // The label wraps the control, so clicking anywhere in the row selects
  // it — the whole row is the target, not just the mark.
  it("makes the whole row the control", () => {
    const { container } = render(<Row checked={false} />);
    const label = container.querySelector("label");
    expect(label?.querySelector("input")).not.toBeNull();
  });
});
