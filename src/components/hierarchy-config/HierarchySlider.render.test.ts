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
  it("renders 32 positions for RV32I and 64 for RV64I", () => {
    const { container: c32 } = render(HierarchySlider, { props: { hierarchy: [], isa: "RV32I" } });
    expect(c32.querySelectorAll("[data-slider-position]")).toHaveLength(32);

    const { container: c64 } = render(HierarchySlider, { props: { hierarchy: [], isa: "RV64I" } });
    expect(c64.querySelectorAll("[data-slider-position]")).toHaveLength(64);
  });

  it("renders a unified level as one marker with a label above the line", () => {
    render(HierarchySlider, { props: { hierarchy: unified, isa: "RV32I" } });
    expect(screen.getByText("1 KB")).toBeTruthy(); // 2^10
    expect(screen.getByLabelText("L1, 1 KB")).toBeTruthy();
  });

  it("renders a harvard level as two markers with i above and d below", () => {
    render(HierarchySlider, { props: { hierarchy: harvard, isa: "RV32I" } });
    expect(screen.getByLabelText("L1 instrucciones, 4 KB")).toBeTruthy();
    expect(screen.getByLabelText("L1 datos, 1 MB")).toBeTruthy();
  });

  it("shades the occupied span of a harvard level", () => {
    const { container } = render(HierarchySlider, { props: { hierarchy: harvard, isa: "RV32I" } });
    const shaded = container.querySelectorAll("[data-occupied-span]");
    expect(shaded).toHaveLength(1);
  });
});
