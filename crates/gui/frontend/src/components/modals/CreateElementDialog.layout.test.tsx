import { page } from "@vitest/browser/context";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { mount, unmountAll, widthOf } from "../../layoutTest";
import {
  CREATE_DIALOG_GUTTER,
  CREATE_DIALOG_WIDTH,
  CreateElementDialog,
  FIELD_COLUMN_MIN,
  KIND_BUTTON_MIN,
} from "./CreateElementDialog";

afterEach(unmountAll);

/**
 * The Add dialog's arrangement, measured in a real browser.
 *
 * Both defects here were invisible to every other layer, because jsdom
 * performs no layout and answers each of these questions with a zero.
 *
 *  - The kind buttons divided the row between them, so five kinds
 *    squeezed "Storage unit" and "Rain gage" onto two lines each while
 *    "Junction" sat in the same box with room to spare.
 *  - Every field took a row of its own, which made a dialog with six of
 *    them taller than some windows while leaving most of its width
 *    empty. A screen is almost always wider than it is tall.
 */

/**
 * A window with room for the dialog at its full width.
 *
 * The project's default is narrower than the dialog asks for, and the
 * dialog clamps to the window rather than overflowing it — correct, and
 * not what most of these are about. The clamp has its own test at the
 * bottom, where the window is the subject rather than the setting.
 */
const ROOMY = { width: 900, height: 900 };

beforeAll(async () => {
  await page.viewport(ROOMY.width, ROOMY.height);
});

const NOOP = () => {};
const SUBMIT = () => Promise.resolve();

function kindsOf(...labels: string[]) {
  return labels.map((label) => ({ value: label.toLowerCase(), label }));
}

function Dialog({
  kinds,
  children,
}: {
  kinds: string[];
  children?: React.ReactNode;
}) {
  return (
    <CreateElementDialog
      open
      title="Add element"
      kinds={kindsOf(...kinds)}
      kind={kinds[0].toLowerCase()}
      onKindChange={NOOP}
      id="D1"
      onIdChange={NOOP}
      onSubmit={SUBMIT}
      onCancel={NOOP}
    >
      {children}
    </CreateElementDialog>
  );
}

/** A field of the shape the modal puts in the grid. */
function Field({ name }: { name: string }) {
  return (
    <div
      data-testid={name}
      style={{ display: "flex", flexDirection: "column" }}
    >
      <span>{name}</span>
      <input aria-label={name} />
    </div>
  );
}

describe("the Add dialog's box", () => {
  it("is the same width whatever it holds", async () => {
    // The width is a declaration, not a consequence. A dialog that sized
    // to its content would jump between kinds — and the kind is chosen
    // inside it, so it would jump under the cursor that chose.
    const bare = await mount(<Dialog kinds={["Junction"]} />);
    const full = await mount(
      <Dialog kinds={["Junction", "Outfall", "Divider", "Storage unit"]}>
        <Field name="Invert elevation" />
        <Field name="Maximum depth" />
        <Field name="Initial depth" />
      </Dialog>,
    );
    expect(widthOf(bare, "[role=dialog]")).toBe(CREATE_DIALOG_WIDTH);
    expect(widthOf(full, "[role=dialog]")).toBe(CREATE_DIALOG_WIDTH);
  });

  it("never squeezes a kind button below what its label needs", async () => {
    // Five kinds is what the drainage node tab offers, and the row used
    // to divide the width by however many there were.
    const host = await mount(
      <Dialog
        kinds={["Junction", "Outfall", "Divider", "Storage unit", "Rain gage"]}
      />,
    );
    const buttons = [...host.querySelectorAll("[aria-pressed]")];
    expect(buttons.length).toBe(5);
    for (const b of buttons) {
      expect(b.getBoundingClientRect().width).toBeGreaterThanOrEqual(
        KIND_BUTTON_MIN,
      );
    }
  });

  it("wraps the kinds onto a second row rather than shrinking them", async () => {
    const host = await mount(
      <Dialog
        kinds={["Junction", "Outfall", "Divider", "Storage unit", "Rain gage"]}
      />,
    );
    const tops = [...host.querySelectorAll("[aria-pressed]")].map(
      (b) => b.getBoundingClientRect().top,
    );
    // More than one distinct top edge: they did not all fit on one line,
    // and what gave was the row rather than the labels.
    expect(new Set(tops).size).toBeGreaterThan(1);
  });
});

describe("the Add dialog's fields", () => {
  it("puts two on a row", async () => {
    // The complaint this answers: one field per row made the dialog tall
    // and narrow on a screen that is neither.
    const host = await mount(
      <Dialog kinds={["Junction"]}>
        <Field name="Invert elevation" />
        <Field name="Maximum depth" />
      </Dialog>,
    );
    const first = host.querySelector('[data-testid="Invert elevation"]');
    const second = host.querySelector('[data-testid="Maximum depth"]');
    expect(first?.getBoundingClientRect().top).toBe(
      second?.getBoundingClientRect().top,
    );
  });

  it("gives each column room for a number and its unit", async () => {
    const host = await mount(
      <Dialog kinds={["Junction"]}>
        <Field name="Invert elevation" />
        <Field name="Maximum depth" />
      </Dialog>,
    );
    expect(
      widthOf(host, '[data-testid="Invert elevation"]'),
    ).toBeGreaterThanOrEqual(FIELD_COLUMN_MIN);
  });

  it("still stacks a third field under the first two", async () => {
    // Two columns, not two fields: the grid keeps filling rows.
    const host = await mount(
      <Dialog kinds={["Junction"]}>
        <Field name="A" />
        <Field name="B" />
        <Field name="C" />
      </Dialog>,
    );
    const top = (name: string) =>
      host.querySelector(`[data-testid="${name}"]`)?.getBoundingClientRect()
        .top ?? 0;
    expect(top("A")).toBe(top("B"));
    expect(top("C")).toBeGreaterThan(top("A"));
  });
});

describe("the Add dialog in a narrow window", () => {
  it("clamps to the window rather than overflowing it", async () => {
    // Below the width it asks for, the dialog gives — a modal wider than
    // the window it is centred in has a Cancel button nobody can reach.
    await page.viewport(360, 700);
    const host = await mount(<Dialog kinds={["Junction"]} />);
    expect(widthOf(host, "[role=dialog]")).toBe(360 - CREATE_DIALOG_GUTTER);
    await page.viewport(ROOMY.width, ROOMY.height);
  });

  it("drops to one column rather than squeezing two", async () => {
    // Two columns is the point of the width, but a number with a unit
    // beside it needs room: below that, one good column beats two bad
    // ones.
    await page.viewport(360, 700);
    const host = await mount(
      <Dialog kinds={["Junction"]}>
        <Field name="Invert elevation" />
        <Field name="Maximum depth" />
      </Dialog>,
    );
    const top = (name: string) =>
      host.querySelector(`[data-testid="${name}"]`)?.getBoundingClientRect()
        .top ?? 0;
    expect(top("Maximum depth")).toBeGreaterThan(top("Invert elevation"));
    await page.viewport(ROOMY.width, ROOMY.height);
  });
});
