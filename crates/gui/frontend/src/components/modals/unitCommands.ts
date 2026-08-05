/**
 * The palette's units commands.
 *
 * Theme has had three palette commands since the palette existed; units,
 * the app's other display preference, had none. Adding them is not quite
 * the copy it looks like, because a theme is one global thing and units
 * are two: an app-wide default, and a per-project override that may or may
 * not be set.
 *
 * Which of those the palette offers depends on where the user is, and this
 * function is that decision — named and testable rather than a condition
 * buried in a command array.
 */

import type { DynamicCommand } from "../../types/ui";
import type { UnitPreference } from "../../units";

/** How each preference reads in a command label. */
const NAME: Record<UnitPreference, string> = {
  source: "Source",
  si: "SI (metric)",
  us: "US customary",
};

const ORDER: UnitPreference[] = ["source", "si", "us"];

/**
 * The units commands to offer.
 *
 * With no project open, only the app-wide default is offered: there is
 * nothing to override, and the default is the only unit decision that
 * means anything with nothing loaded.
 *
 * With a project open, only that project's units are offered — deliberately
 * not both. Six near-identical commands would make each of them harder to
 * find than three, and a global default is both the rarer thing to change
 * from inside a project and the one whose effect reaches outside it.
 * Settings is itself one palette command away, and that is where a
 * preference with app-wide reach belongs.
 *
 * @param inProject   whether a project is open.
 * @param projectPref the project's override, or `null` if it follows the
 *                    default.
 * @param defaultPref the app-wide default.
 */
export function unitCommands(
  inProject: boolean,
  projectPref: UnitPreference | null,
  defaultPref: UnitPreference,
): DynamicCommand[] {
  if (!inProject) {
    return ORDER.map((pref) => ({
      id: `a-units-default-${pref}`,
      label: `Default units: ${NAME[pref]}`,
      description:
        pref === defaultPref
          ? "Current default"
          : "For projects that do not set their own",
      category: "Actions",
      action: `units-default-${pref}` as DynamicCommand["action"],
    }));
  }

  const commands: DynamicCommand[] = ORDER.map((pref) => ({
    id: `a-units-project-${pref}`,
    label: `Units: ${NAME[pref]}`,
    description:
      pref === projectPref ? "Current" : "Show this project in these units",
    category: "Actions",
    action: `units-project-${pref}` as DynamicCommand["action"],
  }));

  // The way back to inheriting. Without it the palette could set an
  // override but never clear one, which would make it a one-way door and
  // send the user to Settings for the other half of a decision it just
  // offered to make.
  commands.push({
    id: "a-units-project-inherit",
    label: `Units: Follow my default (${NAME[defaultPref]})`,
    description:
      projectPref === null ? "Current" : "Stop overriding for this project",
    category: "Actions",
    action: "units-project-inherit",
  });

  return commands;
}
