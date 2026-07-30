import { describe, expect, it } from "vitest";
import { inpIdError } from "./inpId";

describe("inpIdError", () => {
  it("accepts ids that survive an INP round trip", () => {
    for (const id of ["J-42", "Test_Pattern", "1", "pump.a", "  J1  "]) {
      expect(inpIdError(id)).toBeNull();
    }
  });

  it("rejects empty and whitespace-only ids", () => {
    expect(inpIdError("")).toBe("ID must not be empty");
    expect(inpIdError("   ")).toBe("ID must not be empty");
  });

  it("rejects interior whitespace, which INP cannot represent", () => {
    // The bug this guards: "Test Pattern" was written to [PATTERNS] verbatim,
    // and read back as id "Test" with a first multiplier of "Pattern".
    expect(inpIdError("Test Pattern")).toBe("ID must not contain spaces");
    expect(inpIdError("a\tb")).toBe("ID must not contain spaces");
  });

  it("rejects the comment marker and quotes", () => {
    for (const id of ["a;b", 'a"b', "a'b"]) {
      expect(inpIdError(id)).toBe("ID must not contain “ ; ” or quotes");
    }
  });

  it("matches the backend rules the dialogs rely on", () => {
    // Trimming is the backend's behaviour too: a padded id is accepted and
    // stored trimmed, not rejected.
    expect(inpIdError(" J1 ")).toBeNull();
  });
});
