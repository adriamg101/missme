import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";
import LevelTabs from "./LevelTabs.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

beforeEach(() => setActivePinia(createPinia()));

describe("LevelTabs", () => {
  it("shows only the RAM tab when the hierarchy is empty and VM is off", () => {
    render(LevelTabs);
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["RAM"]);
  });

  it("shows a tab per cache level plus RAM, split into i/d for harvard levels", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.createLevelAt(15);
    store.convertToHarvard(store.hierarchy[1].id);
    render(LevelTabs);
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["L1", "L2i", "L2d", "RAM"]);
  });

  it("shows TLB and DISK tabs only when VM is enabled, and RAM alongside them", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.setVmEnabled(true);
    render(LevelTabs);
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["TLB", "L1", "RAM", "DISK"]);
  });

  it("switches the visible form when a different tab is clicked", async () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.setVmEnabled(true);
    render(LevelTabs);
    await fireEvent.click(screen.getByText("TLB"));
    expect(screen.getByLabelText("Page size")).toBeTruthy();
    await fireEvent.click(screen.getByText("L1"));
    expect(screen.getByLabelText("Associativity")).toBeTruthy();
  });
});
