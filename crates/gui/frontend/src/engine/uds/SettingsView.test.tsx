/**
 * @vitest-environment jsdom
 */
/**
 * The drainage settings body: an editable Timing group saved through the
 * engine's own writer.
 *
 * The defect this guards: this body shipped as a read-only summary with
 * a sentence saying editing was "not available yet", written before the
 * editing contract landed and never revisited after it did — so the
 * RESULTS-EMPTY warning sent modellers to a dialog that could not fix
 * what it named.
 */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UdsSimParams } from "../../hooks";
import { UdsSettingsView } from "./SettingsView";

const PARAMS: UdsSimParams = {
  startDate: { year: 2004, month: 1, day: 1 },
  startTime: 0,
  endDate: { year: 2004, month: 1, day: 1 },
  endTime: 0,
  reportStep: 900,
  routingStep: 20,
  minRoutingStep: 0,
  courantFactor: 0,
  wetStep: 300,
  dryStep: 3600,
  flowUnits: "CFS",
  routing: "DYNWAVE",
  infiltration: "HORTON",
};

const getUdsSimParams = vi.fn<(id: string) => Promise<UdsSimParams | null>>(
  () => Promise.resolve(PARAMS),
);
const updateUdsSimParams = vi.fn<
  (id: string, p: UdsSimParams) => Promise<void>
>(() => Promise.resolve());

vi.mock("../../hooks", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getUdsSimParams: (id: string) => getUdsSimParams(id),
  updateUdsSimParams: (id: string, p: UdsSimParams) =>
    updateUdsSimParams(id, p),
}));
vi.mock("../../hooks/scenarios", () => ({
  useScenarios: () => [{ id: "sc1", name: "Storm" }],
}));
const showToast = vi.fn();
const markEdited = vi.fn();
const closeSimSettingsModal = vi.fn();
vi.mock("../../AppContext", () => ({
  useAppState: () => ({
    closeSimSettingsModal,
    showToast,
    bumpSimParams: vi.fn(),
  }),
}));
vi.mock("../../hooks/NetworkVersionContext", () => ({
  useNetworkVersion: () => ({ markEdited }),
}));

async function renderView() {
  render(<UdsSettingsView projectId="p1" />);
  await waitFor(() => expect(screen.queryByText("Loading…")).toBeNull());
}

beforeEach(() => {
  vi.clearAllMocks();
  updateUdsSimParams.mockImplementation(() => Promise.resolve());
});

describe("UdsSettingsView", () => {
  it("offers the timing and the model choices for editing", async () => {
    await renderView();
    // The timing, named rather than counted: a count says a control was
    // added or removed and not which, and this assertion existed to say
    // the timing is editable at all.
    for (const label of [
      "Start date",
      "Start time",
      "End date",
      "End time",
      "Report step (min)",
      "Routing step (s)",
      "Smallest routing step (s)",
      "Courant factor",
      "Wet step (min)",
      "Dry step (min)",
    ]) {
      expect(screen.getByText(label), `${label} is offered`).toBeTruthy();
    }
    expect(document.querySelectorAll("input").length).toBe(10);
    // The choices: real selects, not read-only text.
    const selects = [...document.querySelectorAll("select")].map(
      (s) => (s as HTMLSelectElement).value,
    );
    expect(selects).toEqual(["CFS", "DYNWAVE", "HORTON"]);
    // And no sentence claiming editing is unavailable.
    expect(screen.queryByText(/not available/)).toBeNull();
  });

  it("saves a flipped routing form", async () => {
    await renderView();
    const routing = document.querySelectorAll("select")[1] as HTMLSelectElement;
    fireEvent.change(routing, { target: { value: "KINWAVE" } });
    fireEvent.click(screen.getByLabelText("Save simulation settings"));
    await waitFor(() => expect(updateUdsSimParams).toHaveBeenCalled());
    expect(updateUdsSimParams).toHaveBeenCalledWith("p1", {
      ...PARAMS,
      routing: "KINWAVE",
    });
  });

  it("saves the edited timing, whole, and marks every target stale", async () => {
    await renderView();
    const endTime = document.querySelectorAll(
      'input[type="time"]',
    )[1] as HTMLInputElement;
    fireEvent.change(endTime, { target: { value: "06:00" } });
    fireEvent.click(screen.getByLabelText("Save simulation settings"));

    await waitFor(() => expect(updateUdsSimParams).toHaveBeenCalled());
    expect(updateUdsSimParams).toHaveBeenCalledWith("p1", {
      ...PARAMS,
      endTime: 6 * 3600,
    });
    // Base and the scenario both go stale — settings ride every INP.
    expect(markEdited).toHaveBeenCalledWith("p1", null);
    expect(markEdited).toHaveBeenCalledWith("p1", "sc1");
    expect(closeSimSettingsModal).toHaveBeenCalled();
  });

  it("holds a refusal beside the fields instead of toasting it", async () => {
    updateUdsSimParams.mockImplementation(() =>
      Promise.reject("the run has to end after it starts"),
    );
    await renderView();
    const endTime = document.querySelectorAll(
      'input[type="time"]',
    )[1] as HTMLInputElement;
    fireEvent.change(endTime, { target: { value: "00:01" } });
    fireEvent.click(screen.getByLabelText("Save simulation settings"));

    expect(await screen.findByText(/end after it starts/)).toBeDefined();
    expect(showToast).not.toHaveBeenCalled();
    expect(closeSimSettingsModal).not.toHaveBeenCalled();
  });

  it("offers no save until something changed", async () => {
    await renderView();
    const save = screen.getByLabelText(
      "Save simulation settings",
    ) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
  });
});
