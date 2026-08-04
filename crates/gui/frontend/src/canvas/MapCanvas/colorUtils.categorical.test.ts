import { describe, expect, it } from "vitest";
import type { RampHint } from "../../hooks/results";
import {
  categoryRgba,
  genericRgba,
  SEVERITY_RGB,
  statusRgba,
} from "./colorUtils";

/** Unjudged states: a partition with no abnormal member. */
const MATERIAL: RampHint = {
  type: "categorical",
  items: [
    { value: 0, label: "Concrete" },
    { value: 1, label: "Ductile iron" },
    { value: 2, label: "PVC" },
  ],
};

/** Judged states, as the wds engine publishes link status. */
const STATUS: RampHint = {
  type: "categorical",
  items: [
    { value: 2, label: "Closed", severity: "alarm" },
    { value: 3, label: "Open", severity: "nominal" },
    { value: 4, label: "Active", severity: "caution" },
  ],
};

const v = (ramp: RampHint) => ({ min: 0, max: 10, ramp });

describe("categorical colouring", () => {
  // The stored value is a code, not a position: status 2 is the *first*
  // declared state. Treating the code as an index would paint every link
  // with a colour from the wrong state.
  it("colours by declared position, not by the stored code", () => {
    expect(genericRgba(0, v(MATERIAL))).toEqual(categoryRgba(0));
    expect(genericRgba(1, v(MATERIAL))).toEqual(categoryRgba(1));
    expect(genericRgba(2, v(MATERIAL))).toEqual(categoryRgba(2));
  });

  it("gives each declared state a distinct colour", () => {
    const seen = new Set(
      MATERIAL.type === "categorical"
        ? MATERIAL.items.map((i) => genericRgba(i.value, v(MATERIAL)).join())
        : [],
    );
    expect(seen.size).toBe(3);
  });

  // A code the engine never declared cannot be named, so it must not
  // borrow a neighbouring state's colour and read as that state.
  it("renders an undeclared state as absent", () => {
    const undeclared = genericRgba(9, v(MATERIAL));
    for (let i = 0; i < 3; i += 1) {
      expect(undeclared).not.toEqual(categoryRgba(i));
    }
  });

  // Severity is the stronger claim: the engine has said this state is
  // wrong, which a position in a list could never say. Colouring by
  // position would paint a closed pipe as merely the first kind of link.
  it("colours a judged state by its severity, not its position", () => {
    expect(genericRgba(2, v(STATUS)).slice(0, 3)).toEqual(SEVERITY_RGB.alarm);
    expect(genericRgba(3, v(STATUS)).slice(0, 3)).toEqual(SEVERITY_RGB.nominal);
    expect(genericRgba(4, v(STATUS)).slice(0, 3)).toEqual(SEVERITY_RGB.caution);
  });

  // Reordering a catalog must not repaint the network. Under positional
  // colouring it would.
  it("is stable under a reordering of the declared states", () => {
    const reordered: RampHint = {
      type: "categorical",
      items: [...(STATUS.type === "categorical" ? STATUS.items : [])].reverse(),
    };
    for (const code of [2, 3, 4]) {
      expect(genericRgba(code, v(reordered))).toEqual(
        genericRgba(code, v(STATUS)),
      );
    }
  });

  // The legend draws from the catalog and the wds canvas draws from its
  // fixed period arrays. If those disagree, the swatch a reader looks up
  // does not match the link they looked it up for.
  it("agrees with the wds canvas path on every shared state", () => {
    for (const code of [2, 3, 4]) {
      expect(statusRgba(code).slice(0, 3)).toEqual(
        genericRgba(code, v(STATUS)).slice(0, 3),
      );
    }
  });

  it("ignores the numeric range entirely", () => {
    expect(genericRgba(3, { min: 0, max: 10, ramp: STATUS })).toEqual(
      genericRgba(3, { min: -500, max: 500, ramp: STATUS }),
    );
  });

  it("keeps continuous ramps working alongside", () => {
    const seq = genericRgba(5, v({ type: "sequential" }));
    const div = genericRgba(5, v({ type: "diverging" }));
    expect(seq).not.toEqual(div);
    expect(seq).toHaveLength(4);
  });
});
