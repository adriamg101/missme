import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/vue";
import PercentStat from "./PercentStat.vue";

describe("PercentStat", () => {
  it("shows the percentage to one decimal place", () => {
    render(PercentStat, { props: { label: "Hit rate", value: 0.876 } });
    expect(screen.getByText("87.6%")).toBeTruthy();
  });

  it("fills the bar to match the fraction", () => {
    const { container } = render(PercentStat, { props: { label: "Hit rate", value: 0.5 } });
    const fill = container.querySelector(".stat-cell-bar-fill") as HTMLElement;
    expect(fill.style.width).toBe("50%");
  });

  it("clamps a value above 1 to 100%", () => {
    render(PercentStat, { props: { label: "Hit rate", value: 1.5 } });
    expect(screen.getByText("100.0%")).toBeTruthy();
  });
});
