/** @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * What the tab bar offers and what each tab shows. The categories are
 * engine-authored strings riding the block DTOs (hydra-common §3.2); the
 * view derives its tabs from them, so these tests feed blocks and read
 * buttons — no engine, no backend.
 *
 * `AppProvider` cannot mount under jsdom (it registers Tauri listeners),
 * so the two app hooks are mocked rather than provided.
 */

// Mutable so the run-stamps tests can vary what the metadata carries;
// hoisted because the mock factory runs before module bodies do.
const sim = vi.hoisted(() => ({
  resultGeneration: 0,
  resultMeta: null as { startedAtMs?: number; finishedAtMs?: number } | null,
}));

vi.mock("../../AppContext", () => ({
  useAppState: () => ({ activeProjectId: "p1", activeScenarioId: null }),
  useSimulation: () => sim,
}));

vi.mock("../../hooks/ipc", () => ({ tryInvokeOr: vi.fn() }));

import { tryInvokeOr } from "../../hooks/ipc";
import { BlockAnalysisView } from "./BlockAnalysisView";
import type { AnalysisBlock } from "./fragments";

const invoke = vi.mocked(tryInvokeOr);
const serve = (blocks: AnalysisBlock[]) =>
  invoke.mockImplementation(async () => blocks);

const block = (id: string, title: string, category: string): AnalysisBlock => ({
  id,
  title,
  category,
  status: "ok",
  fragment: { title, items: [{ type: "note", text: `${title} body` }] },
});

const TWO_TABS = [
  block("wds.run-summary", "Run Summary", "Summary"),
  block("wds.service-compliance", "Pressure Adequacy", "Compliance"),
];

beforeEach(() => {
  sim.resultMeta = null;
});

describe("BlockAnalysisView tabs", () => {
  it("offers one tab per category and starts on the first", async () => {
    serve(TWO_TABS);
    render(<BlockAnalysisView />);
    expect(await screen.findByRole("button", { name: "Summary" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Compliance" })).toBeTruthy();
    expect(screen.getByText("Run Summary body")).toBeTruthy();
    expect(screen.queryByText("Pressure Adequacy body")).toBeNull();
  });

  it("switching tabs swaps which blocks render", async () => {
    serve(TWO_TABS);
    render(<BlockAnalysisView />);
    fireEvent.click(await screen.findByRole("button", { name: "Compliance" }));
    expect(screen.getByText("Pressure Adequacy body")).toBeTruthy();
    expect(screen.queryByText("Run Summary body")).toBeNull();
  });

  it("a single category wears no tab bar", async () => {
    serve([block("uds.run-summary", "Run Summary", "Summary")]);
    render(<BlockAnalysisView />);
    expect(await screen.findByText("Run Summary body")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Summary" })).toBeNull();
  });

  it("an empty result explains itself rather than showing bare tabs", async () => {
    serve([]);
    render(<BlockAnalysisView />);
    expect(
      await screen.findByText("Run a simulation to see results here."),
    ).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Summary" })).toBeNull();
  });
});

describe("BlockAnalysisView run stamps", () => {
  // Local noon instants: the label renders in the environment's locale,
  // so the assertions match on the stable "Ran …" prefix.
  const STAMPED = {
    startedAtMs: new Date(2026, 7, 23, 12, 0, 0).getTime(),
    finishedAtMs: new Date(2026, 7, 23, 12, 0, 30).getTime(),
  };

  it("shows when the run happened, on the overview tab only", async () => {
    sim.resultMeta = STAMPED;
    serve(TWO_TABS);
    render(<BlockAnalysisView />);
    expect(await screen.findByText(/^Ran /)).toBeTruthy();

    // The stamps describe the whole run, not a category: they sit beside
    // the engine's own summary and nowhere else.
    fireEvent.click(screen.getByRole("button", { name: "Compliance" }));
    expect(screen.queryByText(/^Ran /)).toBeNull();
  });

  it("stays silent for results that predate the stamps", async () => {
    sim.resultMeta = {};
    serve(TWO_TABS);
    render(<BlockAnalysisView />);
    expect(await screen.findByText("Run Summary body")).toBeTruthy();
    expect(screen.queryByText(/^Ran /)).toBeNull();
  });

  it("stays silent when there are no blocks to describe", async () => {
    sim.resultMeta = STAMPED;
    serve([]);
    render(<BlockAnalysisView />);
    expect(
      await screen.findByText("Run a simulation to see results here."),
    ).toBeTruthy();
    expect(screen.queryByText(/^Ran /)).toBeNull();
  });
});
