// src/components/hierarchy-config/HierarchySlider.render.test.ts
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/vue";
import HierarchySlider from "./HierarchySlider.vue";
import type { Hierarchy } from "../../lib/hierarchy/model";
import { defaultCacheConfig } from "../../lib/hierarchy/model";

const unified: Hierarchy = [
  { id: "a", harvard: false, unified: { exp: 10, config: defaultCacheConfig() }, inclusion: "inclusive", busWidth: 64 },
];

const harvard: Hierarchy = [
  {
    id: "b",
    harvard: true,
    i: { exp: 12, config: defaultCacheConfig() },
    d: { exp: 20, config: defaultCacheConfig() },
    inclusion: "inclusive",
    busWidth: 64,
  },
];

describe("HierarchySlider rendering", () => {
  it("renders positions 0..31 for RV32I and 0..63 for RV64I", () => {
    const { container: c32 } = render(HierarchySlider, { props: { hierarchy: [], isa: "RV32I" } });
    expect(c32.querySelectorAll("[data-slider-position]")).toHaveLength(32);

    const { container: c64 } = render(HierarchySlider, { props: { hierarchy: [], isa: "RV64I" } });
    expect(c64.querySelectorAll("[data-slider-position]")).toHaveLength(64);
  });

  it("renders a unified level as one marker with a label above the line", () => {
    render(HierarchySlider, { props: { hierarchy: unified, isa: "RV32I" } });
    expect(screen.getByText("1 K")).toBeTruthy(); // 2^10
    expect(screen.getByLabelText("L1, 1 K")).toBeTruthy();
  });

  it("renders a harvard level as two markers with i above and d below", () => {
    render(HierarchySlider, { props: { hierarchy: harvard, isa: "RV32I" } });
    expect(screen.getByLabelText("L1 instrucciones, 4 K")).toBeTruthy();
    expect(screen.getByLabelText("L1 datos, 1 M")).toBeTruthy();
  });

  it("shows the level index inside each marker, with an i/d suffix for harvard levels", () => {
    render(HierarchySlider, { props: { hierarchy: harvard, isa: "RV32I" } });
    expect(screen.getByLabelText("L1 instrucciones, 4 K").textContent).toContain("1i");
    expect(screen.getByLabelText("L1 datos, 1 M").textContent).toContain("1d");
  });

  it("labels power-of-two ruler positions plus position 0 and the maximum", () => {
    const { container } = render(HierarchySlider, { props: { hierarchy: [], isa: "RV32I" } });
    const labeled = Array.from(container.querySelectorAll(".hierarchy-slider-tick-label")).map((el) => el.textContent);
    expect(labeled.sort((a, b) => Number(a) - Number(b))).toEqual(["0", "1", "2", "4", "8", "16", "31"]);
  });
});
