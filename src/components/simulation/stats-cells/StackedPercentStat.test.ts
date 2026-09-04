import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/vue";
import StackedPercentStat from "./StackedPercentStat.vue";

describe("StackedPercentStat", () => {
  it("renders one legend entry per segment with its percentage", () => {
    render(StackedPercentStat, {
      props: {
        label: "Cycle share",
        segments: [
          { label: "L1", value: 0.6 },
          { label: "RAM", value: 0.4 },
        ],
      },
    });
    expect(screen.getByText("L1")).toBeTruthy();
    expect(screen.getByText("60.0%")).toBeTruthy();
    expect(screen.getByText("RAM")).toBeTruthy();
    expect(screen.getByText("40.0%")).toBeTruthy();
  });

  it("renders one bar segment per data segment", () => {
    const { container } = render(StackedPercentStat, {
      props: {
        label: "Cycle share",
        segments: [
          { label: "L1", value: 0.3 },
          { label: "L2", value: 0.3 },
          { label: "RAM", value: 0.4 },
        ],
      },
    });
    expect(container.querySelectorAll(".stat-cell-stacked-segment").length).toBe(3);
  });
});
