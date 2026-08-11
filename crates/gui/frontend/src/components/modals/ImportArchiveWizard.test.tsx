/** @vitest-environment jsdom */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

/**
 * The archive review, as the user reads it: which rows can be selected,
 * what the create button promises, and — after a create — which rows say
 * "Created" and which carry their own failure. Everything here is what
 * the modal *shows*; the decisions behind it are tested as data in
 * `archiveImport.test.ts`.
 */

const { createProjectsFromArchive } = vi.hoisted(() => ({
  createProjectsFromArchive: vi.fn(),
}));

vi.mock("../../hooks", async () => {
  const actual =
    await vi.importActual<typeof import("../../hooks")>("../../hooks");
  return {
    ...actual,
    createProjectsFromArchive,
    // The registry hook needs no backend for labels.
    useEngines: () => [
      { key: "wds", label: "Water Distribution" },
      { key: "uds", label: "Urban Drainage" },
    ],
  };
});

import type { ArchiveScan } from "../../hooks";
import { ImportArchiveWizard } from "./ImportArchiveWizard";

const SCAN: ArchiveScan = {
  archivePath: "/tmp/models.zip",
  others: ["rain.dat"],
  models: [
    {
      path: "a/water.inp",
      stem: "water",
      engine: "wds",
      candidates: [],
      nodeCount: 12,
      linkCount: 14,
      findingCount: 0,
      repairs: [],
      sidecars: [],
      error: null,
    },
    {
      path: "a/drainage.inp",
      stem: "drainage",
      engine: "uds",
      candidates: [],
      nodeCount: 5,
      linkCount: 4,
      findingCount: 0,
      repairs: [],
      sidecars: [
        {
          file: "rain.dat",
          label: 'rain file "rain.dat"',
          carried: true,
          supported: true,
        },
      ],
      error: null,
    },
    {
      path: "a/noise.inp",
      stem: "noise",
      engine: null,
      candidates: [],
      nodeCount: 0,
      linkCount: 0,
      findingCount: 0,
      repairs: [],
      sidecars: [],
      error: "no engine recognises this file",
    },
  ],
};

describe("ImportArchiveWizard", () => {
  it("offers the importable rows and refuses the failed one", () => {
    render(
      <ImportArchiveWizard scan={SCAN} onClose={() => {}} onDone={() => {}} />,
    );
    expect(
      (
        screen.getByRole("checkbox", {
          name: "Import a/water.inp",
        }) as HTMLInputElement
      ).checked,
    ).toBe(true);
    // The failed entry is visible — the user must see what will not
    // import and why — but never selectable.
    expect(
      (
        screen.getByRole("checkbox", {
          name: "Import a/noise.inp",
        }) as HTMLInputElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByText("no engine recognises this file")).toBeTruthy();
    // The button promises exactly the included count.
    expect(
      (
        screen.getByRole("button", {
          name: "Create 2 projects",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    // What the archive carries but the import does not.
    expect(
      screen.getByText("Not imported (not model files): rain.dat"),
    ).toBeTruthy();
  });

  it("unticking a row changes what the create button promises", () => {
    render(
      <ImportArchiveWizard scan={SCAN} onClose={() => {}} onDone={() => {}} />,
    );
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Import a/drainage.inp" }),
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Create 1 project",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });

  it("reports each outcome against its own row after creating", async () => {
    createProjectsFromArchive.mockResolvedValue([
      {
        path: "a/water.inp",
        name: "water",
        project: { id: "p1" },
        error: null,
      },
      {
        path: "a/drainage.inp",
        name: "drainage",
        project: null,
        error: "disk full",
      },
    ]);
    const onDone = vi.fn();
    render(
      <ImportArchiveWizard scan={SCAN} onClose={() => {}} onDone={onDone} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Create 2 projects" }));

    await waitFor(() => {
      expect(screen.getByText("Created")).toBeTruthy();
    });
    // Partial success is loudly reported, row by row, not rolled back.
    expect(screen.getByText("disk full")).toBeTruthy();
    expect(
      screen.getByText("1 of 2 selected models became projects."),
    ).toBeTruthy();
    // The names travel with the create call.
    expect(createProjectsFromArchive).toHaveBeenCalledWith("/tmp/models.zip", [
      { path: "a/water.inp", name: "water", engine: "wds" },
      { path: "a/drainage.inp", name: "drainage", engine: "uds" },
    ]);
    // Done, not Cancel, is the way out now — and it reports the count.
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(onDone).toHaveBeenCalledWith(1);
  });
});
