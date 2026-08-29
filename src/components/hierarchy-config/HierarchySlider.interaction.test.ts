// src/components/hierarchy-config/HierarchySlider.interaction.test.ts
import { describe, it, expect } from "vitest";
import { render, fireEvent } from "@testing-library/vue";
import HierarchySlider from "./HierarchySlider.vue";
import type { Hierarchy } from "../../lib/hierarchy/model";
import { defaultCacheConfig } from "../../lib/hierarchy/model";

const oneLevel: Hierarchy = [
  { id: "a", harvard: false, unified: { exp: 10, config: defaultCacheConfig() }, inclusion: "inclusive", busWidth: 64 },
];

describe("HierarchySlider interactions", () => {
  it("emits create with the clicked position on an empty slot", async () => {
    const { container, emitted } = render(HierarchySlider, { props: { hierarchy: [], isa: "RV32I" } });
    const slot = container.querySelectorAll("[data-slider-position]")[5];
    await fireEvent.click(slot!);
    expect(emitted().create[0]).toEqual([5]);
  });

  it("emits rejected-insert instead of create when clicking on an occupied position", async () => {
    const { container, emitted } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    const slot = container.querySelectorAll("[data-slider-position]")[10];
    await fireEvent.click(slot!);
    expect(emitted().create).toBeUndefined();
    expect(emitted()["rejected-insert"][0]).toEqual([["a"]]);
  });

  it("a plain click on an existing marker (below the drag threshold) does not emit move", async () => {
    const { getByLabelText, emitted } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    const marker = getByLabelText("L1, 1 K");
    await fireEvent.mouseDown(marker, { clientX: 100 });
    await fireEvent.mouseUp(marker, { clientX: 101 }); // 1px, below the 4px threshold
    expect(emitted().move).toBeUndefined();
  });

  it("dragging a marker past the threshold emits move with the target position", async () => {
    const { getByLabelText, emitted } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    const marker = getByLabelText("L1, 1 K");
    await fireEvent.mouseDown(marker, { clientX: 100 });
    await fireEvent.mouseMove(document, { clientX: 130 }); // past the 4px threshold
    await fireEvent.mouseUp(document, { clientX: 130 });
    expect(emitted().move).toBeTruthy();
    const moveEvents = emitted<[string, string, number]>("move");
    expect(moveEvents[0][0]).toBe("a");
    expect(moveEvents[0][1]).toBe("unified");
  });
});
