/**
 * @vitest-environment jsdom
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DeleteConfirmModal } from "./DeleteConfirmModal";

/**
 * The cascade warning is the one sentence standing between a user and an
 * unexpected removal, and it used to be decided here from a set of kind
 * names — "junction", "reservoir", "tank". That is one engine's
 * vocabulary in a shared component: it warned for a drainage junction by
 * coincidence and said nothing for an outfall, a storage unit or a
 * divider, each of which takes its conduits with it.
 *
 * The caller decides now, because the caller knows the element's class.
 * These tests pin the warning to the prop rather than to any name.
 */

const props = {
  open: true,
  elementId: "J1",
  onConfirm: vi.fn(),
  onCancel: vi.fn(),
};

const CASCADE = /connected links will also be removed/;

describe("DeleteConfirmModal", () => {
  it("warns about the cascade when the caller says there is one", () => {
    render(
      <DeleteConfirmModal {...props} elementKind="outfall" takesLinks={true} />,
    );
    expect(screen.getByText(CASCADE, { exact: false })).toBeDefined();
  });

  it("stays quiet when nothing cascades", () => {
    render(
      <DeleteConfirmModal
        {...props}
        elementKind="conduit"
        takesLinks={false}
      />,
    );
    expect(screen.queryByText(CASCADE, { exact: false })).toBeNull();
  });

  it("does not decide from the kind name", () => {
    // The exact pairing that was wrong: a kind this dialog once
    // recognised, deleted in a way that takes nothing with it.
    render(
      <DeleteConfirmModal
        {...props}
        elementKind="junction"
        takesLinks={false}
      />,
    );
    expect(screen.queryByText(CASCADE, { exact: false })).toBeNull();
  });

  it("says nothing about a cascade by default", () => {
    // An omitted prop must not be read as "yes": a caller that has not
    // thought about it should under-claim, not warn about a removal
    // that will not happen.
    render(<DeleteConfirmModal {...props} elementKind="junction" />);
    expect(screen.queryByText(CASCADE, { exact: false })).toBeNull();
  });

  it("names the element being deleted", () => {
    render(<DeleteConfirmModal {...props} elementKind="junction" />);
    expect(screen.getByText("J1")).toBeDefined();
  });
});
