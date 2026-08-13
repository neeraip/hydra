/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  type EditorSection,
  EditorShell,
  EditorStatusBar,
} from "./EditorShell";

const sections: EditorSection[] = [
  { id: "junctions", label: "Junctions", count: 1204, kindId: "junction" },
  { id: "pipes", label: "Pipes", count: 1876, kindId: "pipe", dirtyCount: 1 },
  {
    id: "patterns",
    label: "Patterns",
    count: 6,
    startsGroup: true,
    groupLabel: "Patterns and curves",
  },
];

const renderShell = (over: Partial<Parameters<typeof EditorShell>[0]> = {}) => {
  const onSelectSection = vi.fn();
  const utils = render(
    <EditorShell
      sections={sections}
      activeSectionId="junctions"
      onSelectSection={onSelectSection}
      {...over}
    >
      <div>body</div>
    </EditorShell>,
  );
  return { ...utils, onSelectSection };
};

describe("EditorShell", () => {
  // The rail is the model's inventory: every kind and its size, visible
  // without a click. The horizontal tab strip it replaced could scroll
  // entries out of view with no affordance that more existed.
  it("lists every section with its count", () => {
    renderShell();
    for (const s of sections) {
      expect(screen.getByText(s.label)).toBeDefined();
      expect(screen.getByText(String(s.count))).toBeDefined();
    }
  });

  it("marks the active section for assistive tech, not just visually", () => {
    renderShell();
    const active = screen.getByText("Junctions").closest("button");
    expect(active?.getAttribute("aria-current")).toBe("page");
    const other = screen.getByText("Pipes").closest("button");
    expect(other?.getAttribute("aria-current")).toBeNull();
  });

  it("reports the section the user picked", () => {
    const { onSelectSection } = renderShell();
    fireEvent.click(screen.getByText("Pipes"));
    expect(onSelectSection).toHaveBeenCalledWith("pipes");
  });

  // Staged-but-unsaved work has to be findable without opening each
  // section in turn.
  it("marks sections holding unsaved work", () => {
    renderShell();
    expect(screen.getAllByLabelText("unsaved changes")).toHaveLength(1);
  });

  it("renders the body and the footer", () => {
    renderShell({ footer: <EditorStatusBar>status</EditorStatusBar> });
    expect(screen.getByText("body")).toBeDefined();
    expect(screen.getByText("status")).toBeDefined();
  });

  // An engine with no editing still gets a bar in the same place, so the
  // page does not end in a different silhouette per engine.
  it("renders a footer even with nothing staged", () => {
    renderShell({
      footer: <EditorStatusBar>Read-only</EditorStatusBar>,
    });
    expect(screen.getByText("Read-only")).toBeDefined();
  });

  // A shorthand and one of its longhands in the same inline style object
  // is a silent trap: React writes keys in order and assigns "" for
  // undefined, so a conditional `paddingTop: undefined` after
  // `padding: "8px 14px"` *removed* the top padding rather than leaving
  // it. Every ordinary row rendered flush against its top edge.
  it("pads every row evenly, top and bottom", () => {
    renderShell();
    for (const label of ["Junctions", "Pipes"]) {
      const row = screen.getByText(label).closest("button");
      const style = (row as HTMLButtonElement).style;
      expect(style.paddingTop).not.toBe("");
      expect(style.paddingTop).toBe(style.paddingBottom);
    }
  });

  // The divider used to be the row's own top border, which put the rule
  // and the gap above the label inside the button's box: the row lit up
  // and was clickable when the pointer was over what reads as empty space
  // between two groups.
  it("draws the group divider outside every row", () => {
    const { container } = renderShell();
    // `<hr>` carries the separator role implicitly, so the divider needs
    // no ARIA of its own.
    const divider = container.querySelector("hr");
    expect(divider).not.toBeNull();
    expect(divider?.closest("button")).toBeNull();
  });

  it("pads a group-starting row exactly like any other", () => {
    renderShell();
    const grouped = (
      screen.getByText("Patterns").closest("button") as HTMLButtonElement
    ).style;
    const plain = (
      screen.getByText("Junctions").closest("button") as HTMLButtonElement
    ).style;
    expect(grouped.paddingTop).toBe(plain.paddingTop);
    expect(grouped.paddingTop).toBe(grouped.paddingBottom);
  });

  it("draws one divider per group, not one per row", () => {
    const { container } = renderShell();
    expect(container.querySelectorAll("hr")).toHaveLength(1);
  });

  // An absent kind and an empty one are different facts. Hiding empties
  // left no way to tell a sparse model from an application that cannot
  // show them — so they are listed, but recede.
  it("dims a section the model has nothing in", () => {
    renderShell({
      sections: [
        { id: "junctions", label: "Junctions", count: 12 },
        { id: "weirs", label: "Weirs", count: 0 },
      ],
      activeSectionId: "junctions",
    });
    const opacity = (label: string) =>
      Number.parseFloat(
        (screen.getByText(label).closest("button") as HTMLButtonElement).style
          .opacity || "1",
      );
    expect(opacity("Weirs")).toBeLessThan(opacity("Junctions"));
  });

  // Opening it and reading "no elements of this kind" is the confirmation
  // the entry exists to give, so it must stay reachable.
  it("keeps an empty section selectable", () => {
    const { onSelectSection } = renderShell({
      sections: [{ id: "weirs", label: "Weirs", count: 0 }],
      activeSectionId: "junctions",
    });
    const button = screen.getByText("Weirs").closest("button");
    expect((button as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(button as HTMLButtonElement);
    expect(onSelectSection).toHaveBeenCalledWith("weirs");
  });

  it("does not dim the active section, empty or not", () => {
    renderShell({
      sections: [{ id: "weirs", label: "Weirs", count: 0 }],
      activeSectionId: "weirs",
    });
    const style = (
      screen.getByText("Weirs").closest("button") as HTMLButtonElement
    ).style;
    expect(style.opacity === "" || Number.parseFloat(style.opacity) === 1).toBe(
      true,
    );
  });

  it("survives an engine that has no sections yet", () => {
    expect(() => renderShell({ sections: [] })).not.toThrow();
  });

  it("shows the engine's heading above the entry that opens a group", () => {
    // The rail never learns what a group means — it draws the word the
    // engine supplied, above the entry that opens it.
    render(
      <EditorShell
        sections={sections}
        activeSectionId="junctions"
        onSelectSection={() => {}}
      >
        <div />
      </EditorShell>,
    );
    expect(screen.getByText("Patterns and curves")).toBeDefined();
  });
});
