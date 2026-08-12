import { describe, expect, it } from "vitest";
import { deletionSummary } from "./deletionSummary";

const removed = (
  over: Partial<Parameters<typeof deletionSummary>[0]> = {},
) => ({
  id: "J2",
  links: [],
  attachments: [],
  ...over,
});

describe("deletionSummary", () => {
  it("says nothing when only the element went", () => {
    // The canvas already shows it gone; a toast repeating what the
    // screen just did is noise.
    expect(deletionSummary(removed())).toBeNull();
  });

  it("names the links that went with a node", () => {
    expect(deletionSummary(removed({ links: ["C1"] }))).toBe(
      "Deleted J2 and C1.",
    );
  });

  it("counts links rather than listing a crowd of them", () => {
    // A node in a drainage model can have a dozen conduits on it, and a
    // toast naming all twelve is a toast nobody reads.
    expect(deletionSummary(removed({ links: ["A", "B", "C", "D"] }))).toBe(
      "Deleted J2 and 4 links.",
    );
    // The boundary: three still read as a list.
    expect(deletionSummary(removed({ links: ["A", "B", "C"] }))).toBe(
      "Deleted J2 and A, B, C.",
    );
  });

  it("reports the records that only described the element", () => {
    // These are the removals a user does not expect — nothing on screen
    // showed them, so the message is the only place they appear.
    expect(
      deletionSummary(
        removed({ links: ["C1"], attachments: ["2 inflows", "1 treatment"] }),
      ),
    ).toBe("Deleted J2 and C1, 2 inflows and 1 treatment.");
  });

  it("joins the last item with 'and', not a comma", () => {
    expect(deletionSummary(removed({ attachments: ["1 inflow"] }))).toBe(
      "Deleted J2 and 1 inflow.",
    );
    expect(
      deletionSummary(removed({ attachments: ["1 inflow", "1 treatment"] })),
    ).toBe("Deleted J2 and 1 inflow and 1 treatment.");
  });
});
