/** @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Which section each setting is filed under, and the launch-check
 * preference.
 *
 * A settings row is found by where it sits, so the grouping is part of
 * what the row says. Two were saying the wrong thing: display units under
 * Appearance read as a look, when it decides what number you type into a
 * diameter field, and the data folder under About read as part of "what is
 * this program" rather than where your work lives.
 *
 * `AppProvider` cannot mount under jsdom — it registers Tauri listeners —
 * so the app hooks are mocked rather than provided.
 */

const { setAutoUpdateCheck, autoCheck } = vi.hoisted(() => ({
  setAutoUpdateCheck: vi.fn(),
  autoCheck: { value: true },
}));

vi.mock("../../AppContext", () => ({
  useAppState: () => ({
    theme: "dark",
    setTheme: vi.fn(),
    openBasemapProvidersModal: vi.fn(),
  }),
}));

vi.mock("../../hooks", async () => {
  const actual =
    await vi.importActual<typeof import("../../hooks")>("../../hooks");
  return {
    ...actual,
    getVersions: async () => ({ app: "2.14.0", hydra: "9.0.0" }),
    openDataFolder: vi.fn(),
  };
});

vi.mock("../../hooks/useUpdater", () => ({
  useUpdater: () => ({
    updater: { phase: "idle" },
    supported: true,
    install: vi.fn(),
    restart: vi.fn(),
    checkNow: vi.fn(),
  }),
  readAutoUpdateCheck: () => autoCheck.value,
  setAutoUpdateCheck,
}));

import { SettingsContent } from "./SettingsContent";

/** The section heading a row sits under, by document order. */
function sectionOf(container: HTMLElement, label: string): string | null {
  let section: string | null = null;
  for (const child of Array.from(container.children)) {
    if (child.tagName === "H2") {
      section = child.textContent;
      continue;
    }
    if (child.textContent?.includes(label)) return section;
  }
  return null;
}

beforeEach(() => {
  autoCheck.value = true;
  setAutoUpdateCheck.mockClear();
});

describe("where each setting is filed", () => {
  it("puts display units under General, not Appearance", () => {
    // It governs entry and reporting — a convention you work in, not a
    // preference about how the app looks.
    const { container } = render(<SettingsContent />);
    expect(sectionOf(container, "Default display units")).toBe("General");
  });

  it("puts the data folder under Data, not About", () => {
    const { container } = render(<SettingsContent />);
    expect(sectionOf(container, "Data folder")).toBe("Data");
  });

  it("leaves identity and legal under About", () => {
    const { container } = render(<SettingsContent />);
    expect(sectionOf(container, "Licence")).toBe("About");
    expect(sectionOf(container, "Open-source components")).toBe("About");
    expect(sectionOf(container, "Software updates")).toBe("About");
  });

  it("keeps theme and the basemap providers under Appearance", () => {
    const { container } = render(<SettingsContent />);
    expect(sectionOf(container, "Theme")).toBe("Appearance");
    expect(sectionOf(container, "Basemap providers")).toBe("Appearance");
  });
});

describe("the launch update check", () => {
  it("can be declined, and says the manual check still works", () => {
    render(<SettingsContent />);
    const row = screen.getByText("Check for updates automatically")
      .parentElement?.parentElement as HTMLElement;
    expect(row.textContent).toContain("the button below still works");
    const toggle = row.querySelector(
      "input[type=checkbox]",
    ) as HTMLInputElement;
    expect(toggle.checked).toBe(true);
    fireEvent.click(toggle);
    expect(setAutoUpdateCheck).toHaveBeenCalledWith(false);
  });

  it("is offered even when the launch check was skipped", () => {
    // The row is hidden on installs that cannot self-update, and whether
    // they can is settled by the launch check. Skipping it must not also
    // hide the toggle that turns it back on.
    autoCheck.value = false;
    render(<SettingsContent />);
    expect(screen.getByText("Check for updates automatically")).toBeTruthy();
  });
});
