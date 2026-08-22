// src/components/hierarchy-config/HierarchySlider.menu.test.ts
import { describe, it, expect } from "vitest";
import { render, fireEvent, screen } from "@testing-library/vue";
import HierarchySlider from "./HierarchySlider.vue";
import type { Hierarchy } from "../../lib/hierarchy/model";
import { defaultCacheConfig } from "../../lib/hierarchy/model";

const oneLevel: Hierarchy = [
  { id: "a", harvard: false, unified: { exp: 10, config: defaultCacheConfig() }, inclusion: "inclusive", busWidth: 64 },
];

describe("HierarchySlider context menu", () => {
  it("opens on right-click with an option to convert to Harvard, labeled with the level name", async () => {
    const { getByLabelText } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    await fireEvent.contextMenu(getByLabelText("L1, 1 K"));
    expect(screen.getByText("Convertir L1 a Harvard")).toBeTruthy();
    expect(screen.getByText("Eliminar L1")).toBeTruthy();
  });

  it("emits convert-to-harvard when that option is chosen", async () => {
    const { getByLabelText, emitted } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    await fireEvent.contextMenu(getByLabelText("L1, 1 K"));
    await fireEvent.click(screen.getByText("Convertir L1 a Harvard"));
    expect(emitted()["convert-to-harvard"][0]).toEqual(["a"]);
  });

  it("emits remove-level when Eliminar is chosen", async () => {
    const { getByLabelText, emitted } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    await fireEvent.contextMenu(getByLabelText("L1, 1 K"));
    await fireEvent.click(screen.getByText("Eliminar L1"));
    expect(emitted()["remove-level"][0]).toEqual(["a"]);
  });

  it("closes the menu on Escape", async () => {
    render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    await fireEvent.contextMenu(screen.getByLabelText("L1, 1 K"));
    expect(screen.queryByText("Eliminar L1")).toBeTruthy();
    await fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByText("Eliminar L1")).toBeNull();
  });

  it("moves focus with arrow keys, clamped to the same limits as dragging", async () => {
    const { getByLabelText, emitted } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    const marker = getByLabelText("L1, 1 K");
    marker.focus();
    await fireEvent.keyDown(marker, { key: "ArrowRight" });
    expect(emitted().move[0]).toEqual(["a", "unified", 11]);
  });

  it("positions the context menu at the click's viewport coordinates (teleported, not clipped by an ancestor)", async () => {
    const { getByLabelText, container } = render(HierarchySlider, { props: { hierarchy: oneLevel, isa: "RV32I" } });
    await fireEvent.contextMenu(getByLabelText("L1, 1 K"), { clientX: 42, clientY: 84 });
    // teleported to <body>, so it is NOT inside the component's own container
    expect(container.querySelector(".hierarchy-slider-context-menu")).toBeNull();
    const menu = document.querySelector(".hierarchy-slider-context-menu") as HTMLElement;
    expect(menu).toBeTruthy();
    expect(menu.style.left).toBe("42px");
    expect(menu.style.top).toBe("84px");
  });
});
