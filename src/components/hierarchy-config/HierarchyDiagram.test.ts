import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
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

describe("HierarchyDiagram — click animation", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("adds a pressed class when a clickable box is clicked", async () => {
    const { container } = render(HierarchyDiagram);
    const reg = container.querySelector('[data-box-id="reg"]')!;
    expect(reg.classList.contains("pressed")).toBe(false);
    await fireEvent.click(reg);
    expect(reg.classList.contains("pressed")).toBe(true);
  });

  it("does not add a pressed class when CPU (non-clickable) is clicked", async () => {
    const { container } = render(HierarchyDiagram);
    const cpu = container.querySelector('[data-box-id="cpu"]')!;
    await fireEvent.click(cpu);
    expect(cpu.classList.contains("pressed")).toBe(false);
  });

  it("does not add a pressed class when RAM (terminal, no VM) is clicked", async () => {
    const { container } = render(HierarchyDiagram);
    const ram = container.querySelector('[data-box-id="ram"]')!;
    await fireEvent.click(ram);
    expect(ram.classList.contains("pressed")).toBe(false);
  });
});
