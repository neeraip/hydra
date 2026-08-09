/** @vitest-environment jsdom */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BlockPanel } from "./FragmentView";
import type { AnalysisBlock } from "./fragments";

/**
 * What the user actually reads: a block's fragment rendered whole — every
 * item kind including charts, which the first fragment renderer silently
 * dropped (`return null`), so a tank-levels block showed a heading over
 * nothing and read as broken.
 */

const BLOCK: AnalysisBlock = {
  id: "wds.everything",
  title: "Everything",
  category: "Summary",
  status: "ok",
  fragment: {
    title: "Everything",
    items: [
      {
        type: "keyValues",
        entries: [
          {
            label: "Min pressure",
            value: { type: "number", value: 14.2, unit: "psi" },
          },
        ],
      },
      {
        type: "table",
        table: {
          columns: [
            { name: "Junction", kind: "text" },
            { name: "Deficit", unit: "psi", kind: "number" },
          ],
          rows: [
            [
              { type: "text", value: "J17" },
              { type: "number", value: 3.5 },
            ],
          ],
        },
      },
      { type: "note", text: "Sampled from 2048 periods." },
      {
        type: "chart",
        chart: {
          xLabel: "Pressure band",
          yLabel: "Junctions",
          data: {
            type: "bar",
            categories: ["< 20 psi", "≥ 20 psi"],
            values: [2, 40],
          },
        },
      },
      {
        type: "chart",
        chart: {
          xLabel: "Time",
          xUnit: "h",
          yLabel: "Head",
          yUnit: "m",
          data: {
            type: "line",
            series: [
              {
                name: "T1",
                points: [
                  [0, 10],
                  [1, 12],
                ] as [number, number][],
              },
            ],
          },
        },
      },
    ],
  },
};

describe("BlockPanel", () => {
  it("renders every item kind, charts included", () => {
    render(<BlockPanel block={BLOCK} />);
    expect(screen.getByText("14.20 psi")).toBeTruthy();
    expect(screen.getByText("Deficit (psi)")).toBeTruthy();
    expect(screen.getByText("J17")).toBeTruthy();
    expect(screen.getByText("Sampled from 2048 periods.")).toBeTruthy();
    // The bar chart draws its band labels; the line chart names its series.
    expect(screen.getByText("< 20 psi")).toBeTruthy();
    expect(screen.getByText("T1")).toBeTruthy();
  });

  it("an unavailable block shows its engine-authored reason", () => {
    render(
      <BlockPanel
        block={{
          id: "wds.pump-energy",
          title: "Pump Energy",
          category: "Assets",
          status: "unavailable",
          reason: "The network has no pumps.",
        }}
      />,
    );
    expect(screen.getByText("The network has no pumps.")).toBeTruthy();
  });
});
