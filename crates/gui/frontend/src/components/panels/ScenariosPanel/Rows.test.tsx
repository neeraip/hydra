/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ScenarioRow } from "./Rows";
import type { FlatScenario } from "./shared";

function scenario(over: Partial<FlatScenario> = {}): FlatScenario {
  return {
    id: "s1",
    projectId: "p1",
    parentScenarioId: null,
    name: "Storm",
    state: "not-run",
    depth: 0,
    ...over,
  };
}

function renderRow(over: { isResumable?: boolean; onResume?: () => void }) {
  render(
    <ScenarioRow
      scenario={scenario()}
      isActive={false}
      isRenaming={false}
      renameValue=""
      isDeleting={false}
      isRunning={false}
      parentName={null}
      onActivate={() => {}}
      onRenameStart={() => {}}
      onRenameChange={() => {}}
      onRenameCommit={() => {}}
      onRenameCancel={() => {}}
      onBranch={() => {}}
      onRun={() => {}}
      onResume={over.onResume ?? (() => {})}
      isResumable={over.isResumable ?? false}
      onClearResults={() => {}}
      onDelete={() => {}}
      onOpenFolder={() => {}}
    />,
  );
  fireEvent.click(screen.getByLabelText("Actions for Storm"));
}

describe("ScenarioRow — continuing an interrupted run", () => {
  it("offers to continue when there is an interrupted run", () => {
    const onResume = vi.fn();
    renderRow({ isResumable: true, onResume });
    fireEvent.click(screen.getByText("Continue interrupted run"));
    expect(onResume).toHaveBeenCalledTimes(1);
  });

  it("keeps the entry and says why when there is nothing to continue", () => {
    // Inert rather than absent, the way "Clear results" already is: a
    // labelled row can say why it does not apply, and a person who
    // cancelled a run and cannot find how to continue it learns whether
    // the option exists at all.
    const onResume = vi.fn();
    renderRow({ isResumable: false, onResume });
    const entry = screen.getByText("Continue interrupted run");
    fireEvent.click(entry);
    expect(onResume).not.toHaveBeenCalled();
    expect(screen.getByText(/no interrupted run/i)).toBeTruthy();
  });
});
