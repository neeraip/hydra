import { describe, expect, it } from "vitest";
import { unitCommands } from "./unitCommands";

/**
 * Which units commands the palette offers, and where.
 *
 * The decision this pins is that the two unit scopes never appear at once:
 * outside a project only the app-wide default can be set, inside one only
 * that project's units. Nothing else in the app distinguishes them by
 * label alone — "Units: SI" and "Default units: SI" differ by two words —
 * so a regression that offered both would read as a duplicate rather than
 * as a bug.
 */

const labels = (cmds: ReturnType<typeof unitCommands>) =>
  cmds.map((c) => c.label);

describe("the palette's units commands", () => {
  it("offers only the app-wide default with no project open", () => {
    expect(labels(unitCommands(false, null, "source"))).toEqual([
      "Default units: Source",
      "Default units: SI (metric)",
      "Default units: US customary",
    ]);
  });

  it("offers only the project's units with one open", () => {
    const cmds = unitCommands(true, null, "source");
    expect(labels(cmds)).toEqual([
      "Units: Source",
      "Units: SI (metric)",
      "Units: US customary",
      "Units: Follow my default (Source)",
    ]);
    // The scopes are exclusive, stated directly rather than implied by the
    // list above: no project command may leak into the default list.
    expect(labels(cmds).some((l) => l.startsWith("Default units:"))).toBe(
      false,
    );
  });

  /**
   * Setting an override has to be reversible from the same surface that
   * set it, or the palette is a one-way door into a state only Settings
   * can leave.
   */
  it("can clear an override, not only set one", () => {
    const cmds = unitCommands(true, "us", "si");
    const inherit = cmds.find((c) => c.action === "units-project-inherit");
    expect(inherit?.description).toBe("Stop overriding for this project");
  });

  /**
   * The inherit command names what it will resolve to, because "follow my
   * default" alone does not tell the user what they are about to see.
   */
  it("names the default it would fall back to", () => {
    expect(labels(unitCommands(true, "us", "si"))).toContain(
      "Units: Follow my default (SI (metric))",
    );
  });

  /** Each scope marks its own current value, and only its own. */
  it("marks the current setting in either scope", () => {
    const outside = unitCommands(false, null, "us");
    expect(outside.find((c) => c.label.endsWith("US customary"))?.description) //
      .toBe("Current default");

    const inside = unitCommands(true, "si", "us");
    expect(inside.find((c) => c.label === "Units: SI (metric)")?.description) //
      .toBe("Current");
    // The project follows its own override, so the fallback is not current
    // even though the app default is US.
    expect(
      inside.find((c) => c.action === "units-project-inherit")?.description,
    ).not.toBe("Current");
  });

  /** An unset override is itself a state the list has to show as current. */
  it("marks inheriting as current when nothing is overridden", () => {
    const cmds = unitCommands(true, null, "us");
    expect(
      cmds.find((c) => c.action === "units-project-inherit")?.description,
    ).toBe("Current");
    expect(cmds.filter((c) => c.description === "Current")).toHaveLength(1);
  });

  /** Every command carries a distinct id and action, since the palette keys
   *  on the first and switches on the second. */
  it("gives every command a distinct id and action", () => {
    const all = [
      ...unitCommands(false, null, "si"),
      ...unitCommands(true, null, "si"),
    ];
    expect(new Set(all.map((c) => c.id)).size).toBe(all.length);
    expect(new Set(all.map((c) => c.action)).size).toBe(all.length);
  });
});
