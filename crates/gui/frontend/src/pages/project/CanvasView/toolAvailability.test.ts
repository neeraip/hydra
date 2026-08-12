import { describe, expect, it } from "vitest";
import type { CanvasTool } from "../../../canvas/types";
import { toolAllowedBy, toolAvailableIn } from "./toolAvailability";

const ALL: CanvasTool[] = ["select", "measure", "edit", "add-node", "add-link"];

describe("toolAvailableIn", () => {
  it("offers every tool on the map", () => {
    for (const tool of ALL) {
      expect(toolAvailableIn("map", tool)).toBe(true);
    }
  });

  it("withholds the tools that act on a coordinate in the schematic", () => {
    for (const tool of ["edit", "add-node", "measure"] as CanvasTool[]) {
      expect(toolAvailableIn("schematic", tool)).toBe(false);
    }
  });

  it("withholds add-link in the schematic", () => {
    // It carries no coordinates and so once worked there; it is withheld
    // because the schematic redraws itself when connectivity changes,
    // moving the layout under the hand drawing the link. Asserted on its
    // own so a future reader does not restore it on the reasoning that
    // links have no geometry — that part was never the objection.
    expect(toolAvailableIn("schematic", "add-link")).toBe(false);
  });

  it("keeps selection available everywhere", () => {
    // The schematic is still for reading the network, so the tool that
    // reads it has to survive the switch — otherwise resetting to Select
    // on entry would land on a tool that is itself withheld.
    expect(toolAvailableIn("schematic", "select")).toBe(true);
  });
});

describe("toolAllowedBy", () => {
  const both = {
    geometry: true,
    rename: true,
    create: true,
    delete: true,
    title: true,
  };
  const moveOnly = {
    geometry: true,
    rename: true,
    create: false,
    delete: true,
    title: false,
  };
  const neither = {
    geometry: false,
    rename: false,
    create: false,
    delete: false,
    title: false,
  };

  it("offers every tool to an engine that can do everything", () => {
    for (const tool of ALL) {
      expect(toolAllowedBy(both, tool)).toBe(true);
    }
  });

  it("separates moving from creating", () => {
    // The whole reason these are separate capabilities rather than one
    // flag. Drainage can move an element — its position is a line the
    // backend maintains — and cannot create one, because nothing
    // supplies the defaults a new element needs.
    expect(toolAllowedBy(moveOnly, "edit")).toBe(true);
    expect(toolAllowedBy(moveOnly, "add-node")).toBe(false);
    expect(toolAllowedBy(moveOnly, "add-link")).toBe(false);
  });

  it("does not read deleting as permission to create", () => {
    // The pair that used to be one "structure" flag. They ask for
    // opposite things — a default for every field versus every
    // reference found — so an engine that can remove an element is not
    // thereby able to add one.
    expect(moveOnly.delete).toBe(true);
    expect(toolAllowedBy(moveOnly, "add-node")).toBe(false);
  });

  it("withholds every editing tool from an engine that edits nothing", () => {
    expect(toolAllowedBy(neither, "edit")).toBe(false);
    expect(toolAllowedBy(neither, "add-node")).toBe(false);
    expect(toolAllowedBy(neither, "add-link")).toBe(false);
  });

  it("never withholds the tools that ask nothing of the model", () => {
    // A model no one can edit still has a canvas worth reading.
    expect(toolAllowedBy(neither, "select")).toBe(true);
    expect(toolAllowedBy(neither, "measure")).toBe(true);
  });
});
