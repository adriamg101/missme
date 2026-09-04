import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import { mount } from "@vue/test-utils";
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

  it("adds a pressed class when RAM (terminal, no VM) is clicked", async () => {
    const { container } = render(HierarchyDiagram);
    const ram = container.querySelector('[data-box-id="ram"]')!;
    await fireEvent.click(ram);
    expect(ram.classList.contains("pressed")).toBe(true);
  });
});

describe("HierarchyDiagram — box colors", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("gives each box its own --box-hue/--box-chroma custom properties, driving the CSS oklch() color", () => {
    const store = useHierarchyConfigStore();
    store.setVmEnabled(true);
    store.createLevelAt(10);
    const { container } = render(HierarchyDiagram);

    const cpu = container.querySelector('[data-box-id="cpu"]') as HTMLElement;
    expect(cpu.style.getPropertyValue("--box-hue")).toBe("15");
    expect(cpu.style.getPropertyValue("--box-chroma")).toBe("0.07");

    const ram = container.querySelector('[data-box-id="ram"]') as HTMLElement;
    expect(ram.style.getPropertyValue("--box-hue")).toBe("255");

    // Cache level ids are randomly generated, so find the L1 box by its
    // visible label text instead of by a fixed id.
    const cacheGroup = screen.getByText("L1").closest("g")!;
    expect(cacheGroup.style.getPropertyValue("--box-hue")).toBe("45");
    expect(cacheGroup.style.getPropertyValue("--box-chroma")).toBe("0.09");
  });
});

describe("HierarchyDiagram — status line", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("shows no status message until setStatusMessage is called", () => {
    render(HierarchyDiagram);
    expect(screen.queryByText(/Reading|Writing|Fetching/)).toBeNull();
  });

  it("shows a general read/write/fetch message with the hex address", async () => {
    const wrapper = mount(HierarchyDiagram);
    (wrapper.vm as unknown as { setStatusMessage: (kind: string, address: number) => void }).setStatusMessage("load", 0x1000);
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain("Reading data at 0x1000");
  });

  it("clears the status message on clearAccessStates", async () => {
    const wrapper = mount(HierarchyDiagram);
    const vm = wrapper.vm as unknown as { setStatusMessage: (kind: string, address: number) => void; clearAccessStates: () => void };
    vm.setStatusMessage("store", 0x2000);
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain("Writing data at 0x2000");

    vm.clearAccessStates();
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).not.toContain("Writing data at 0x2000");
  });

  it("shows a fetch message", async () => {
    const wrapper = mount(HierarchyDiagram);
    (wrapper.vm as unknown as { setStatusMessage: (kind: string, address: number) => void }).setStatusMessage("fetch", 0x3000);
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain("Fetching instruction at 0x3000");
  });
});

describe("HierarchyDiagram — boxClick emit", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("emits boxClick with the clicked box when it is clickable", async () => {
    const { container, emitted } = render(HierarchyDiagram);
    const clickableBox = container.querySelector('[data-box-id="reg"]');
    expect(clickableBox).toBeTruthy();
    await fireEvent.click(clickableBox!);
    const events = emitted().boxClick;
    expect(events).toBeTruthy();
    expect(((events![0] as unknown[])[0] as unknown as { id: string }).id).toBe("reg");
  });

  it("does not emit boxClick for a non-clickable box", async () => {
    const { container, emitted } = render(HierarchyDiagram);
    const cpuBox = container.querySelector('[data-box-id="cpu"]');
    expect(cpuBox).toBeTruthy();
    await fireEvent.click(cpuBox!);
    expect(emitted().boxClick).toBeFalsy();
  });
});
