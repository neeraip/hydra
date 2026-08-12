import type { CanvasTool, ViewMode } from "../../../canvas/types";
import type { EngineComponents } from "../../../engine/registry";

/**
 * Whether `tool` can be used in `viewMode`.
 *
 * The schematic's positions are synthetic BFS output, not the network's
 * own geometry, so a tool that reads or writes a coordinate has nothing
 * true to act on there: `edit` and `add-node` place one, and `measure`
 * reports a distance between two.
 *
 * `add-link` is here for a different reason. It carries no coordinates —
 * creating a link takes two node ids — so it *works* in the schematic,
 * and it was offered there on the argument that connectivity is easiest
 * to see in a drawn layout. In practice it caused more trouble than it
 * saved: a link drawn against invented positions reads as a statement
 * about where things are, and the schematic redraws itself as soon as
 * the connectivity it is drawing changes, so the layout moves under the
 * hand that is editing it. Placement belongs where the coordinates are
 * real.
 */
export function toolAvailableIn(viewMode: ViewMode, tool: CanvasTool): boolean {
  if (viewMode === "map") return true;
  return (
    tool !== "edit" &&
    tool !== "add-node" &&
    tool !== "add-link" &&
    tool !== "measure"
  );
}

/**
 * Whether the engine's model can have done to it what `tool` does.
 *
 * The pairing is the point: `edit` moves an element, which needs
 * somewhere to put a position; `add-node` and `add-link` create one,
 * which needs a default for every field the new element carries. An
 * engine can have the first without the second, and drainage does.
 *
 * Navigation and measurement ask nothing of the model and are always
 * allowed — a read-only engine still has a canvas worth reading.
 */
export function toolAllowedBy(
  editing: EngineComponents["editing"],
  tool: CanvasTool,
): boolean {
  switch (tool) {
    case "edit":
      return editing.geometry;
    case "add-node":
    case "add-link":
      return editing.create;
    default:
      return true;
  }
}
