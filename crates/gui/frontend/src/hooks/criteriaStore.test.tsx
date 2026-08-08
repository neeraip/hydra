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
  // Mirrors the real wrapper's two outcomes: a command that answered, and
  // one that could not be asked. A mock that only ever answers cannot show
  // the difference the store now turns on.
  tryInvokeResult: async (_cmd: string, args: { projectId: string }) => {
    const value = await getProjectCriteria(args.projectId);
    return value === UNREADABLE ? { ok: false } : { ok: true, value };
  },
}));

/** What the mocked wrapper treats as a failed call. */
const UNREADABLE = Symbol("unreadable");

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

/**
 * A read that fails is not a project without criteria.
 *
 * `saved === false` is the canvas's cue to seed pressure bands from the
 * simulation options and *write* them. A failed read used to arrive as the
 * same `null` a never-saved project does, so it set `saved` to false and
 * the seeding wrote defaults over criteria sitting on disk intact. The
 * in-flight case was already guarded and said so in a comment; this is the
 * same sentence, for the other way of not knowing.
 */
describe("a criteria read that fails", () => {
  it("leaves `saved` unknown rather than saying none are saved", async () => {
    getProjectCriteria.mockResolvedValue(UNREADABLE);
    render(<Reader id="unreadable" tag="canvas" />);
    await waitFor(() => expect(getProjectCriteria).toHaveBeenCalled());
    // `null`, which the seeding effect waits on. `false` would seed.
    await waitFor(() =>
      expect(screen.getByTestId("canvas-saved").textContent).toBe("null"),
    );
  });

  it("still reads as the defaults, so nothing renders empty", async () => {
    getProjectCriteria.mockResolvedValue(UNREADABLE);
    render(<Reader id="unreadable-2" tag="canvas" />);
    await waitFor(() =>
      expect(screen.getByTestId("canvas-value").textContent).toBe(
        String(DEFAULT_CRITERIA.minPressureM),
      ),
    );
  });

  /** A project with genuinely none still seeds — the case that must survive. */
  it("is distinct from a project that has never had any", async () => {
    getProjectCriteria.mockResolvedValue(null);
    render(<Reader id="never-saved" tag="canvas" />);
    await waitFor(() =>
      expect(screen.getByTestId("canvas-saved").textContent).toBe("false"),
    );
  });

  /** Not cached, so the next reader gets a fresh attempt. */
  it("is retried rather than remembered", async () => {
    getProjectCriteria.mockResolvedValue(UNREADABLE);
    const { unmount } = render(<Reader id="retry" tag="canvas" />);
    await waitFor(() => expect(getProjectCriteria).toHaveBeenCalledTimes(1));
    unmount();
    render(<Reader id="retry" tag="analysis" />);
    await waitFor(() => expect(getProjectCriteria).toHaveBeenCalledTimes(2));
  });
});
