import type { CanvasTool, ViewMode } from "../../../canvas/types";

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
