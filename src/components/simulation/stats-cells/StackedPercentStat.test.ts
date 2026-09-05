import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
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

  it("recalculates percentages relative to only the checked levels", () => {
    render(StackedPercentStat, {
      props: {
        label: "Cycle share",
        segments: [
          { label: "L1", value: 0.5 },
          { label: "RAM", value: 0.3 },
          { label: "Disk", value: 0.2 },
        ],
        excluded: ["Disk"],
      },
    });

    // With Disk excluded, L1 and RAM now split 100% between just the two of them.
    expect(screen.getByText("62.5%")).toBeTruthy(); // 0.5 / 0.8
    expect(screen.getByText("37.5%")).toBeTruthy(); // 0.3 / 0.8
    expect(screen.getByText("—")).toBeTruthy();
  });

  it("emits the toggled label so the caller can remember the selection itself", async () => {
    const { emitted } = render(StackedPercentStat, {
      props: {
        label: "Cycle share",
        segments: [
          { label: "L1", value: 0.6 },
          { label: "RAM", value: 0.4 },
        ],
      },
    });

    const checkboxes = screen.getAllByRole("checkbox");
    await fireEvent.click(checkboxes[1]); // uncheck RAM

    expect(emitted()["update:excluded"][0]).toEqual([["RAM"]]);
  });
});
