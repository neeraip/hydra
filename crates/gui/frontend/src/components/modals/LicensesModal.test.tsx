/** @vitest-environment jsdom */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

/**
 * The panel that discharges what a shipped binary owes its user: which
 * licence it is under, where the source is, and the notices of everything
 * it is built from.
 *
 * What must not regress is what a reader can *see* — a package whose
 * notice is only reachable through a button nobody renders has not been
 * reproduced. So this asserts on rendered text rather than on the data
 * behind it.
 */

// Hoisted: `vi.mock` factories run before the file's own statements, so a
// plain const declared here would not exist yet when the factory closes
// over it.
const { getThirdPartyLicenseText } = vi.hoisted(() => ({
  getThirdPartyLicenseText: vi.fn(async () => "MIT License\n\nCopyright…"),
}));

vi.mock("../../hooks/licenses", async () => {
  const actual = await vi.importActual<typeof import("../../hooks/licenses")>(
    "../../hooks/licenses",
  );
  return {
    ...actual,
    getLicenseInfo: async () => ({
      spdx: "AGPL-3.0-only",
      text: "GNU AFFERO GENERAL PUBLIC LICENSE\nVersion 3",
      commercial: "# Hydra Commercial License\n\nMost people do not need one.",
      sourceUrl: "https://github.com/neeraip/hydra",
    }),
    listThirdPartyComponents: async () => [
      {
        name: "serde",
        version: "1.0.219",
        ecosystem: "rust",
        spdx: "MIT OR Apache-2.0",
        url: "https://github.com/serde-rs/serde",
        files: [{ name: "LICENSE-MIT", text: 0 }],
      },
      {
        name: "fxhash",
        version: "0.2.1",
        ecosystem: "rust",
        spdx: "Apache-2.0/MIT",
        url: "https://github.com/cbreeden/fxhash",
        files: [],
      },
    ],
    getThirdPartyLicenseText,
  };
});

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

import { LicensesModal } from "./LicensesModal";

describe("LicensesModal", () => {
  it("names the licence and offers the source before any legalese", async () => {
    render(<LicensesModal tab="hydra" onClose={() => {}} />);
    // The answer first: what it is, and that the licence covers the
    // software rather than the results.
    expect(await screen.findByText(/AGPL-3.0-only/)).toBeTruthy();
    expect(screen.getByRole("button", { name: /Source code/ })).toBeTruthy();
    // The text itself is behind a click — 663 lines is not an opening.
    expect(screen.queryByText(/GNU AFFERO/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Read the full/ }));
    expect(screen.getByText(/GNU AFFERO/)).toBeTruthy();
  });

  it("lists a component and fetches its licence text on demand", async () => {
    render(<LicensesModal tab="components" onClose={() => {}} />);
    expect(await screen.findByText("serde")).toBeTruthy();
    // Nothing is fetched until a text is asked for: a megabyte of licences
    // would otherwise cross the wire to render a list.
    expect(getThirdPartyLicenseText).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "LICENSE-MIT" }));
    expect(getThirdPartyLicenseText).toHaveBeenCalledWith(0);
    await waitFor(() => expect(screen.getByText(/MIT License/)).toBeTruthy());
  });

  it("still states a notice for a package that ships no licence file", async () => {
    render(<LicensesModal tab="components" onClose={() => {}} />);
    // Around eighty packages are in this shape. An empty row would read as
    // a missing notice rather than a package that declares its licence in
    // its manifest and nowhere else.
    const note = await screen.findByText(/ships no licence file/);
    expect(note.textContent).toContain("Apache-2.0/MIT");
  });

  it("narrows the list to what was searched for", async () => {
    render(<LicensesModal tab="components" onClose={() => {}} />);
    expect(await screen.findByText("serde")).toBeTruthy();
    fireEvent.change(screen.getByPlaceholderText(/Search by name/), {
      target: { value: "fxhash" },
    });
    expect(screen.queryByText("serde")).toBeNull();
    expect(screen.getByText("fxhash")).toBeTruthy();
  });

  it("shows the commercial-licence document on its own tab", async () => {
    render(<LicensesModal tab="commercial" onClose={() => {}} />);
    expect(await screen.findByText(/Most people do not need one/)).toBeTruthy();
  });

  it("switches tabs in place, and the way back stays on screen", async () => {
    // The tabs are one panel showing different pages. When this rendered
    // inside the settings drawer it did not look like that: the overlay
    // took the drawer as its frame and the panel's left edge — its title
    // and its first tab — fell outside it, so the other two tabs read as
    // panels stacked on top with no way back.
    render(<LicensesModal tab="hydra" onClose={() => {}} />);
    await screen.findByText(/AGPL-3.0-only/);
    fireEvent.click(
      screen.getByRole("button", { name: "Open-source components" }),
    );
    expect(await screen.findByText("serde")).toBeTruthy();
    expect(document.querySelectorAll('[role="dialog"]').length).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Hydra's licence" }));
    expect(screen.getByText(/AGPL-3.0-only/)).toBeTruthy();
  });

  it("takes Escape from whatever it was opened over", async () => {
    // It is usually opened from inside the settings drawer, whose own
    // Escape handler is registered first: without this, one key closed
    // both and stepping back a level meant losing the drawer too.
    const behind = vi.fn();
    window.addEventListener("keydown", behind);
    const onClose = vi.fn();
    render(<LicensesModal tab="hydra" onClose={onClose} />);
    await screen.findByText(/AGPL-3.0-only/);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
    expect(behind).not.toHaveBeenCalled();
    window.removeEventListener("keydown", behind);
  });
});
