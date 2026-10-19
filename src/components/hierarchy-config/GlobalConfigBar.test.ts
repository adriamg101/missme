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
    // This sets up the same conflict as the "convertToVonNeumann surfaces a
    // conflict without mutating state" test in hierarchyConfig.test.ts, but
    // reached through the store actions instead: L1 harvard(i=10,d=2), L2
    // harvard(i=25,d=6). Merging L1 sets its combined position (and so its
    // "d" value in the chain) to 10, which is bigger than L2's d (6), which
    // breaks the ordering.
    store.createLevelAt(2);
    store.createLevelAt(6);
    store.convertToHarvard(store.hierarchy[1].id);
    store.moveMarker(store.hierarchy[1].id, "i", 25);
    store.convertToHarvard(store.hierarchy[0].id);
    store.moveMarker(store.hierarchy[0].id, "i", 10);

    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
    const { container, getByText } = render(GlobalConfigBar);
    const before = store.hierarchy;

    // markers appear in hierarchy order: L1's i marker, L1's d marker, L2's i marker, L2's d marker
    const l1InstructionMarker = container.querySelectorAll(".hierarchy-slider-marker")[0];
    await fireEvent.contextMenu(l1InstructionMarker!);
    await fireEvent.click(getByText("Convertir L1 a Von Neumann"));

    expect(store.hierarchy).toBe(before);
    expect(alertSpy).toHaveBeenCalled();
    alertSpy.mockRestore();
  });

  it("does not step the page size past a bounded maximum", async () => {
    const store = useHierarchyConfigStore();
    store.setVmEnabled(true);
    store.setPageSize(65536); // already at the intended max
    render(GlobalConfigBar);

    await fireEvent.click(screen.getByLabelText("Increment"));

    expect(useHierarchyConfigStore().pageSize).toBe(65536);
  });

  it("does not step the page size below a bounded minimum", async () => {
    const store = useHierarchyConfigStore();
    store.setVmEnabled(true);
    store.setPageSize(4); // already at the intended min
    render(GlobalConfigBar);

    await fireEvent.click(screen.getByLabelText("Decrement"));

    expect(useHierarchyConfigStore().pageSize).toBe(4);
  });
});
