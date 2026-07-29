import { describe, expect, it } from "vitest";
import { editedKey } from "./NetworkVersionContext";

describe("editedKey", () => {
  it("keeps two projects' base models apart", () => {
    // The bug: the base model's scenario id is `null` in every project, so an
    // unqualified key marked one project's base model edited and every other
    // project's along with it.
    expect(editedKey("project-a", null)).not.toBe(editedKey("project-b", null));
  });

  it("keeps two projects' same-named scenarios apart", () => {
    expect(editedKey("project-a", "s1")).not.toBe(editedKey("project-b", "s1"));
  });

  it("separates a project's base model from its scenarios", () => {
    expect(editedKey("p", null)).not.toBe(editedKey("p", "s1"));
  });

  it("is stable for the same target", () => {
    expect(editedKey("p", "s1")).toBe(editedKey("p", "s1"));
    expect(editedKey("p", null)).toBe(editedKey("p", null));
  });

  it("does not let a scenario id impersonate another project's base model", () => {
    // A scenario literally named "base" must not collide with the sentinel
    // used for the base model of the same project.
    expect(editedKey("p", "base")).not.toBe(editedKey("p", null));
  });
});
