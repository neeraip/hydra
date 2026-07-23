/**
 * Tests for the pipe initial-status column's pure logic: the PipeRow default
 * mapping (hooks/editors.ts) and the select-option → backend patch-value
 * mapping (pipeStatus.ts).
 */
import { describe, expect, it } from "vitest";
import { defaultPipeInitialStatus } from "../../../hooks/editors";
import { PIPE_STATUS_OPTIONS, pipeStatusPatchValue } from "./pipeStatus";
import { buildRowHaystack } from "./tableSearch";

describe("defaultPipeInitialStatus", () => {
  it("passes explicit statuses through", () => {
    expect(defaultPipeInitialStatus("open")).toBe("open");
    expect(defaultPipeInitialStatus("closed")).toBe("closed");
    expect(defaultPipeInitialStatus("cv")).toBe("cv");
  });

  it("defaults undefined (old snapshots / non-pipes) to open", () => {
    expect(defaultPipeInitialStatus(undefined)).toBe("open");
  });
});

describe("pipeStatusPatchValue", () => {
  it("maps row values to the capitalized backend patch values", () => {
    expect(pipeStatusPatchValue("open")).toBe("Open");
    expect(pipeStatusPatchValue("closed")).toBe("Closed");
    expect(pipeStatusPatchValue("cv")).toBe("CV");
  });

  it("covers every select option", () => {
    for (const opt of PIPE_STATUS_OPTIONS) {
      expect(pipeStatusPatchValue(opt.value)).toBe(opt.label);
    }
  });
});

describe("status in the search haystack", () => {
  it("includes initialStatus in a pipe row's haystack", () => {
    const row = {
      id: "P1",
      from: "J1",
      to: "J2",
      length: 10,
      diameter: 100,
      roughness: 100,
      initialStatus: "closed",
      velocity: 0,
      highVelocity: false,
    };
    expect(buildRowHaystack(row)).toContain("closed");
  });
});
