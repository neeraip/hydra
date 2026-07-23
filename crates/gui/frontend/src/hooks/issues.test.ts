import { describe, expect, it } from "vitest";
import {
  countIssues,
  type RunWarning,
  runWarningsToIssues,
  runWarningToIssue,
  type ValidationFinding,
  validationFindingsToIssues,
  validationFindingToIssue,
} from "./issues";

const FIRST_SEEN = "12:34";

describe("validationFindingToIssue", () => {
  it("maps backend 'error' severity to panel 'error'", () => {
    const issue = validationFindingToIssue(
      { severity: "error", code: "NEG-PRESSURE", message: "Negative pressure" },
      FIRST_SEEN,
    );
    expect(issue.severity).toBe("error");
  });

  it("maps backend 'warning' severity to panel 'warn'", () => {
    const issue = validationFindingToIssue(
      { severity: "warning", code: "DEAD-END", message: "Dead-end junction" },
      FIRST_SEEN,
    );
    expect(issue.severity).toBe("warn");
  });

  it("uses the existing 'data' source category", () => {
    const issue = validationFindingToIssue(
      { severity: "warning", code: "X", message: "m" },
      FIRST_SEEN,
    );
    expect(issue.source).toBe("data");
  });

  it("derives a stable id from code + elementId (dismissals persist)", () => {
    const f: ValidationFinding = {
      severity: "error",
      code: "DISCONNECTED",
      message: "Node not connected",
      elementId: "J-12",
      elementKind: "junction",
    };
    const a = validationFindingToIssue(f, "10:00");
    const b = validationFindingToIssue(f, "11:00");
    expect(a.id).toBe(b.id);
    expect(a.id).toBe("validation-DISCONNECTED-J-12");
  });

  it("falls back to 'network' scope when elementId is absent", () => {
    const issue = validationFindingToIssue(
      { severity: "error", code: "NO-SOURCE", message: "No supply source" },
      FIRST_SEEN,
    );
    expect(issue.id).toBe("validation-NO-SOURCE-network");
  });

  it("mentions the element in the detail when present", () => {
    const issue = validationFindingToIssue(
      {
        severity: "warning",
        code: "ZERO-LENGTH",
        message: "Zero-length pipe",
        elementId: "P-3",
        elementKind: "pipe",
      },
      FIRST_SEEN,
    );
    expect(issue.detail).toContain("pipe P-3");
    expect(issue.title).toBe("Zero-length pipe");
  });

  it("starts undismissed with the provided firstSeen", () => {
    const issue = validationFindingToIssue(
      { severity: "error", code: "C", message: "m" },
      FIRST_SEEN,
    );
    expect(issue.dismissed).toBe(false);
    expect(issue.firstSeen).toBe(FIRST_SEEN);
  });
});

describe("validationFindingsToIssues", () => {
  it("maps every finding and counts by mapped severity", () => {
    const issues = validationFindingsToIssues(
      [
        { severity: "error", code: "A", message: "a" },
        { severity: "warning", code: "B", message: "b" },
        { severity: "warning", code: "C", message: "c", elementId: "T1" },
      ],
      FIRST_SEEN,
    );
    expect(issues).toHaveLength(3);
    const counts = countIssues(issues);
    expect(counts.error).toBe(1);
    expect(counts.warn).toBe(2);
    expect(counts.info).toBe(0);
  });

  it("returns [] for an empty findings list", () => {
    expect(validationFindingsToIssues([], FIRST_SEEN)).toEqual([]);
  });
});

describe("runWarningToIssue", () => {
  it("always maps to severity 'warn' and source 'runtime'", () => {
    const issue = runWarningToIssue(
      { code: "NEG-PRESSURE", message: "Negative pressures", elementId: null },
      FIRST_SEEN,
    );
    expect(issue.severity).toBe("warn");
    expect(issue.source).toBe("runtime");
    expect(issue.code).toBe("NEG-PRESSURE");
  });

  it("derives a stable simwarn id from code + elementId (dismissals persist)", () => {
    const w: RunWarning = {
      code: "PUMP-LIMIT",
      message: "Pump cannot deliver head",
      elementId: "PU-7",
    };
    const a = runWarningToIssue(w, "10:00");
    const b = runWarningToIssue(w, "11:00");
    expect(a.id).toBe(b.id);
    expect(a.id).toBe("simwarn-PUMP-LIMIT-PU-7");
  });

  it("falls back to 'network' scope when elementId is null", () => {
    const issue = runWarningToIssue(
      { code: "UNBALANCED", message: "System unbalanced", elementId: null },
      FIRST_SEEN,
    );
    expect(issue.id).toBe("simwarn-UNBALANCED-network");
  });

  it("passes the solver message through, naming the element in the detail", () => {
    const withElement = runWarningToIssue(
      { code: "X", message: "Valve caused ill-conditioning", elementId: "V2" },
      FIRST_SEEN,
    );
    expect(withElement.title).toBe("Valve caused ill-conditioning");
    expect(withElement.detail).toBe(
      "Valve caused ill-conditioning (element V2)",
    );
    const withoutElement = runWarningToIssue(
      { code: "X", message: "System unbalanced", elementId: null },
      FIRST_SEEN,
    );
    expect(withoutElement.detail).toBe("System unbalanced");
  });

  it("starts undismissed with the provided firstSeen", () => {
    const issue = runWarningToIssue(
      { code: "C", message: "m", elementId: null },
      FIRST_SEEN,
    );
    expect(issue.dismissed).toBe(false);
    expect(issue.firstSeen).toBe(FIRST_SEEN);
  });
});

describe("runWarningsToIssues", () => {
  it("maps every warning and counts them all as warns", () => {
    const issues = runWarningsToIssues(
      [
        { code: "A", message: "a", elementId: null },
        { code: "B", message: "b", elementId: "J1" },
      ],
      FIRST_SEEN,
    );
    expect(issues).toHaveLength(2);
    const counts = countIssues(issues);
    expect(counts.warn).toBe(2);
    expect(counts.error).toBe(0);
    expect(counts.info).toBe(0);
  });

  it("returns [] for an empty warnings list", () => {
    expect(runWarningsToIssues([], FIRST_SEEN)).toEqual([]);
  });
});
