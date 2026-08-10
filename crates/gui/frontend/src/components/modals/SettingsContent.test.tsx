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

const { setAutoUpdateCheck, autoCheck, toggleShortcutCard, clearAllResults } =
  vi.hoisted(() => ({
    setAutoUpdateCheck: vi.fn(),
    autoCheck: { value: true },
    toggleShortcutCard: vi.fn(),
    clearAllResults: vi.fn(async () => ({ removed: 3, skipped: 0 })),
  }));

vi.mock("../../hooks/storage", async () => {
  const actual = await vi.importActual<typeof import("../../hooks/storage")>(
    "../../hooks/storage",
  );
  return {
    ...actual,
    getDataUsage: async () => ({
      totalBytes: 1024 ** 3 * 4,
      resultsBytes: 1024 ** 3 * 3,
      projectCount: 2,
    }),
    clearAllResults,
  };
});

vi.mock("../../AppContext", () => ({
  useAppState: () => ({
    theme: "dark",
    setTheme: vi.fn(),
    openBasemapProvidersModal: vi.fn(),
    toggleShortcutCard,
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

import { diagnosticsText, SettingsContent } from "./SettingsContent";

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
  clearAllResults.mockClear();
  toggleShortcutCard.mockClear();
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

describe("the Data section", () => {
  it("says how much is stored and how much of it is results", async () => {
    const { container } = render(<SettingsContent />);
    // The figure arrives from the backend, so the row starts by admitting
    // it does not have one yet.
    expect(sectionOf(container, "Data folder")).toBe("Data");
    expect(
      await screen.findByText(/of which 3.0 GB is simulation results/),
    ).toBeTruthy();
  });

  it("asks before clearing, and says what it did", async () => {
    render(<SettingsContent />);
    await screen.findByText(/simulation results/);
    fireEvent.click(screen.getByRole("button", { name: "Clear results…" }));
    // The affirmative says what it will do, not "OK".
    expect(clearAllResults).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Clear them" }));
    expect(clearAllResults).toHaveBeenCalled();
    expect(await screen.findByText("3 results cleared.")).toBeTruthy();
  });

  it("lets the confirmation be backed out of", async () => {
    render(<SettingsContent />);
    await screen.findByText(/simulation results/);
    fireEvent.click(screen.getByRole("button", { name: "Clear results…" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(clearAllResults).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Clear results…" })).toBeTruthy();
  });
});

describe("the other new rows", () => {
  it("offers the shortcut card, which was only reachable by shortcut", () => {
    render(<SettingsContent />);
    const row = screen.getByText("Keyboard shortcuts").parentElement
      ?.parentElement as HTMLElement;
    fireEvent.click(row.querySelector("button") as HTMLButtonElement);
    expect(toggleShortcutCard).toHaveBeenCalled();
  });

  it("states every call the app makes, and what it never sends", () => {
    const { container } = render(<SettingsContent />);
    const row = screen.getByText("What Hydra sends").parentElement
      ?.parentElement as HTMLElement;
    expect(sectionOf(container, "What Hydra sends")).toBe("About");
    expect(row.textContent).toContain("GitHub");
    expect(row.textContent).toContain("map tiles");
    expect(row.textContent).toContain("never uploaded");
  });

  it("asks before resetting preferences", () => {
    render(<SettingsContent />);
    fireEvent.click(screen.getByRole("button", { name: "Reset…" }));
    expect(
      screen.getByRole("button", { name: "Reset everything" }),
    ).toBeTruthy();
  });
});

describe("diagnosticsText", () => {
  it("carries the three things a bug report is asked for", () => {
    const text = diagnosticsText(
      { app: "2.14.0", hydra: "9.0.0", platform: "macos/aarch64" },
      "Mozilla/5.0 (Macintosh) AppleWebKit/605",
    );
    expect(text).toContain("Hydra 2.14.0 (engine 9.0.0)");
    expect(text).toContain("Platform: macos/aarch64");
    expect(text).toContain("Webview: Mozilla/5.0");
  });

  it("says what it does not know rather than printing nothing", () => {
    // The versions call can fail; a paste reading "Hydra  (engine )" would
    // look like a filled-in report with the numbers rubbed out.
    expect(diagnosticsText(null, "agent")).toContain("Hydra unknown");
    expect(diagnosticsText(null, "agent")).toContain("Platform: unknown");
  });
});
