/** @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SidecarChecklist } from "./SidecarChecklist";

/**
 * What the wizard's data-file checklist shows: a carried reference reads
 * as travelling with the project, a missing one warns and offers Locate,
 * and a model with no references shows nothing at all.
 */

describe("SidecarChecklist", () => {
  it("renders nothing for a model with no references", () => {
    const { container } = render(
      <SidecarChecklist sidecars={[]} busy={false} onLocate={() => {}} />,
    );
    expect(container.innerHTML).toBe("");
  });

  it("answers each reference: included, or missing with a way in", () => {
    const onLocate = vi.fn();
    render(
      <SidecarChecklist
        busy={false}
        onLocate={onLocate}
        sidecars={[
          {
            file: "rain.dat",
            label: 'rain file "rain.dat"',
            carried: true,
            supported: true,
          },
          {
            file: "climate.txt",
            label: 'climate file "climate.txt"',
            carried: false,
            supported: true,
          },
        ]}
      />,
    );
    expect(screen.getByText('rain file "rain.dat"')).toBeTruthy();
    expect(screen.getByText("will be imported")).toBeTruthy();
    // The missing one warns that runs will refuse, and Locate asks for it.
    expect(screen.getByText(/simulations will\s+refuse to run/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Locate…" }));
    expect(onLocate).toHaveBeenCalledTimes(1);
  });

  it("holds the Locate buttons while a dialog is open", () => {
    render(
      <SidecarChecklist
        busy={true}
        onLocate={() => {}}
        sidecars={[
          {
            file: "a.dat",
            label: 'rain file "a.dat"',
            carried: false,
            supported: true,
          },
        ]}
      />,
    );
    expect(
      (screen.getByRole("button", { name: "Locate…" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});

describe("unsupported references", () => {
  it("are named without a Locate button or a promise", () => {
    render(
      <SidecarChecklist
        busy={false}
        onLocate={() => {}}
        sidecars={[
          {
            file: "ext.dat",
            label: 'data series file "ext.dat"',
            carried: true,
            supported: false,
          },
        ]}
      />,
    );
    expect(screen.getByText('data series file "ext.dat"')).toBeTruthy();
    expect(screen.getByText("not supported yet")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Locate…" })).toBeNull();
    expect(screen.queryByText("will be imported")).toBeNull();
  });
});
