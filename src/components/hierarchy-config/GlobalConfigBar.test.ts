import { describe, it, expect, beforeEach, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import GlobalConfigBar from "./GlobalConfigBar.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

beforeEach(() => setActivePinia(createPinia()));

describe("GlobalConfigBar", () => {
  it("toggling the VM checkbox updates the store", async () => {
    render(GlobalConfigBar);
    await fireEvent.click(screen.getByLabelText("Virtual Memory"));
    expect(useHierarchyConfigStore().vmEnabled).toBe(true);
  });

  it("typing a seed updates the store as a number", async () => {
    render(GlobalConfigBar);
    await fireEvent.update(screen.getByPlaceholderText("fully random"), "42");
    expect(useHierarchyConfigStore().seed).toBe(42);
  });

  it("clearing the seed field resets the store value to null", async () => {
    const store = useHierarchyConfigStore();
    store.setSeed(42);
    render(GlobalConfigBar);
    await fireEvent.update(screen.getByPlaceholderText("fully random"), "");
    expect(useHierarchyConfigStore().seed).toBeNull();
  });

  it("clicking an empty slider position creates a level in the store", async () => {
    const { container } = render(GlobalConfigBar);
    const slot = container.querySelectorAll("[data-slider-position]")[5];
    await fireEvent.click(slot!);
    expect(useHierarchyConfigStore().hierarchy).toHaveLength(1);
  });

  it("alerts and does not mutate the hierarchy when converting to Von Neumann would conflict", async () => {
    const store = useHierarchyConfigStore();
    // mirrors the reachable-via-store-actions conflict scenario from
    // hierarchyConfig.test.ts's "convertToVonNeumann surfaces a conflict without mutating state":
    // L1 harvard(i=10,d=2), L2 harvard(i=25,d=6) -> merging L1 sets its unified position (and
    // thus its "d" value in the chain) to 10, which is greater than L2's d (6), breaking the chain.
    store.createLevelAt(2);
    store.createLevelAt(6);
    store.convertToHarvard(store.hierarchy[1].id);
    store.moveMarker(store.hierarchy[1].id, "i", 25);
    store.convertToHarvard(store.hierarchy[0].id);
    store.moveMarker(store.hierarchy[0].id, "i", 10);

    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
    const { container, getByText } = render(GlobalConfigBar);
    const before = store.hierarchy;

    // markers are rendered in hierarchy order: L1's i marker, L1's d marker, L2's i marker, L2's d marker
    const l1InstructionMarker = container.querySelectorAll(".hierarchy-slider-marker")[0];
    await fireEvent.contextMenu(l1InstructionMarker!);
    await fireEvent.click(getByText("Convertir L1 a Von Neumann"));

    expect(store.hierarchy).toBe(before);
    expect(alertSpy).toHaveBeenCalled();
    alertSpy.mockRestore();
  });
});
