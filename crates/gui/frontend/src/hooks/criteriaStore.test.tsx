// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Every project view is mounted at once — hidden with `display: none`
 * rather than unmounted — so the Analysis page and the canvas are both live
 * whichever one you are looking at, and both read the criteria.
 *
 * Held in component state they got a copy each: two fetches of the same
 * file on open, and, once both could edit them, a change on one side the
 * other never saw. The canvas went on colouring by the bands it loaded
 * with. That is a value stored twice, which is the defect shape this app
 * keeps producing, so it is worth a test that two readers cannot drift.
 */

const getProjectCriteria = vi.fn();
const updateProjectCriteria = vi.fn();

vi.mock("./ipc", () => ({
  invoke: (...args: unknown[]) => updateProjectCriteria(...args),
  tryInvokeOr: (_cmd: string, args: { projectId: string }) =>
    getProjectCriteria(args.projectId),
}));

const { DEFAULT_CRITERIA, useProjectCriteria } = await import("./criteria");

function Reader({ id, tag }: { id: string; tag: string }) {
  const { criteria, setCriteria, saved } = useProjectCriteria(id);
  return (
    <div>
      <span data-testid={`${tag}-value`}>{criteria.minPressureM}</span>
      <span data-testid={`${tag}-saved`}>{String(saved)}</span>
      <button
        type="button"
        onClick={() => setCriteria({ ...criteria, minPressureM: 99 })}
      >
        {`edit-${tag}`}
      </button>
    </div>
  );
}

beforeEach(() => {
  getProjectCriteria.mockReset();
  updateProjectCriteria.mockReset();
  getProjectCriteria.mockResolvedValue({
    ...DEFAULT_CRITERIA,
    minPressureM: 20,
  });
});

describe("two readers of one project's criteria", () => {
  /**
   * The load-bearing one. Two components, one edit, and the reader that
   * did not make it has to see it — that is the whole difference between
   * one store and two.
   */
  it("see the same value after either one edits", async () => {
    render(
      <>
        <Reader id="p1" tag="canvas" />
        <Reader id="p1" tag="analysis" />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("canvas-value").textContent).toBe("20"),
    );
    screen.getByText("edit-canvas").click();
    await waitFor(() =>
      expect(screen.getByTestId("analysis-value").textContent).toBe("99"),
    );
    expect(screen.getByTestId("canvas-value").textContent).toBe("99");
  });

  /** Mounting in the same frame, both find nothing cached. Only one asks. */
  it("fetch the file once between them", async () => {
    render(
      <>
        <Reader id="p2" tag="canvas" />
        <Reader id="p2" tag="analysis" />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("canvas-value").textContent).toBe("20"),
    );
    expect(getProjectCriteria).toHaveBeenCalledTimes(1);
  });

  it("agree on whether anything is saved yet", async () => {
    render(
      <>
        <Reader id="p3" tag="canvas" />
        <Reader id="p3" tag="analysis" />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("canvas-saved").textContent).toBe("true"),
    );
    expect(screen.getByTestId("analysis-saved").textContent).toBe("true");
  });

  /** One project's ruler must never be applied to another's network. */
  it("keep different projects apart", async () => {
    getProjectCriteria.mockImplementation((id: string) =>
      Promise.resolve({
        ...DEFAULT_CRITERIA,
        minPressureM: id === "a" ? 11 : 22,
      }),
    );
    render(
      <>
        <Reader id="a" tag="canvas" />
        <Reader id="b" tag="analysis" />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("analysis-value").textContent).toBe("22"),
    );
    expect(screen.getByTestId("canvas-value").textContent).toBe("11");
  });
});
