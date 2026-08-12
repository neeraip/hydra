/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CreateElementDialog } from "./CreateElementDialog";

const KINDS = [
  { value: "junction", label: "Junction" },
  { value: "outfall", label: "Outfall" },
];

type Props = Parameters<typeof CreateElementDialog>[0];

function dialog(over: Partial<Props> = {}) {
  const props: Props = {
    open: true,
    title: "Add node",
    kinds: KINDS,
    kind: "junction",
    onKindChange: vi.fn(),
    id: "J1",
    onIdChange: vi.fn(),
    onSubmit: vi.fn(() => Promise.resolve()),

    onCancel: vi.fn(),
    ...over,
  };
  render(<CreateElementDialog {...props} />);
  return props;
}

const add = () => screen.getByRole("button", { name: /add/i });

describe("CreateElementDialog", () => {
  it("creates on Add", async () => {
    const props = dialog();
    fireEvent.click(add());
    await vi.waitFor(() => expect(props.onSubmit).toHaveBeenCalled());
  });

  it("keeps itself open with the reason when the create is refused", async () => {
    // The refusals this dialog exists to carry are the interesting ones:
    // a kind that needs a curve, an id already in use. Closing on
    // failure would lose both the message and everything typed.
    dialog({
      onSubmit: vi.fn(() =>
        Promise.reject(new Error("ID 'J1' is already in use")),
      ),
    });
    fireEvent.click(add());
    await vi.waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("already in use"),
    );
    expect(screen.getByRole("dialog")).toBeDefined();
  });

  it("refuses an id the file format cannot carry", () => {
    // A space or a semicolon breaks INP tokenisation on the next save,
    // whichever engine wrote it — so the check is here, where every
    // create passes, rather than in each engine's form.
    dialog({ id: "J 1" });
    expect(screen.getByRole("alert")).toBeDefined();
    expect(add()).toHaveProperty("disabled", true);
  });

  it("does not call an empty field an error", () => {
    // A field nobody has typed in yet is not a mistake. It still cannot
    // be submitted.
    dialog({ id: "" });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(add()).toHaveProperty("disabled", true);
  });

  it("does not offer a choice of one", () => {
    // A row of one button reads as though the others are still loading.
    dialog({
      kinds: [{ value: "conduit", label: "Conduit" }],
      kind: "conduit",
    });
    expect(screen.queryByRole("button", { name: "Conduit" })).toBeNull();
    expect(screen.getByText("Conduit")).toBeDefined();
  });

  it("marks which kind is chosen", () => {
    dialog();
    expect(
      screen
        .getByRole("button", { name: "Junction" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(
      screen
        .getByRole("button", { name: "Outfall" })
        .getAttribute("aria-pressed"),
    ).toBe("false");
  });

  it("submits on Enter in the id field", () => {
    const props = dialog();
    fireEvent.keyDown(screen.getByLabelText("ID"), { key: "Enter" });
    expect(props.onSubmit).toHaveBeenCalled();
  });

  it("renders nothing when closed", () => {
    dialog({ open: false });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
