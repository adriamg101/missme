import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/vue";
import { createPinia, setActivePinia } from "pinia";
import HierarchyDiagram from "./HierarchyDiagram.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

describe("HierarchyDiagram", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("renders CPU, REG and RAM by default (no VM, no cache levels)", () => {
    render(HierarchyDiagram);
    expect(screen.getByText("CPU")).toBeTruthy();
    expect(screen.getByText("REG")).toBeTruthy();
    expect(screen.getByText("RAM")).toBeTruthy();
    expect(screen.queryByText("DISK")).toBeNull();
    expect(screen.queryByText("TLB")).toBeNull();
  });

  it("shows TLB and DISK once VM is enabled", async () => {
    const store = useHierarchyConfigStore();
    store.setVmEnabled(true);
    render(HierarchyDiagram);
    expect(screen.getByText("TLB")).toBeTruthy();
    expect(screen.getByText("DISK")).toBeTruthy();
  });

  it("shows a box per cache level, reacting to the store", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(10);
    render(HierarchyDiagram);
    expect(screen.getByText("L1")).toBeTruthy();
  });
});
