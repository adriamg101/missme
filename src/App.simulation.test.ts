import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, waitFor } from "@testing-library/vue";

let allHandlers: Array<(event: unknown) => void> = [];

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockImplementation((_event: string, handler: (event: unknown) => void) => {
    allHandlers.push(handler);
    return Promise.resolve(() => {});
  }),
}));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));

import App from "./App.vue";
import { useSimulationStore } from "./stores/simulation";
import { useHierarchyConfigStore } from "./stores/hierarchyConfig";

beforeEach(() => {
  setActivePinia(createPinia());
  allHandlers = [];
});

describe("App simulation panel swap", () => {
  it("shows the hierarchy config panel by default", () => {
    render(App);
    expect(document.querySelector(".global-config-bar")).toBeTruthy();
  });

  it("shows the simulation info panel while a simulation is active", () => {
    const store = useSimulationStore();
    store.isActive = true;
    render(App);
    expect(document.querySelector(".simulation-info-panel")).toBeTruthy();
  });

  it("clicking the register box while active opens the registers view", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const { container } = render(App);
    const regBox = container.querySelector('[data-box-id="reg"]');
    expect(regBox).toBeTruthy();
    await (regBox as HTMLElement).dispatchEvent(new Event("click", { bubbles: true }));
    expect(store.activeView).toEqual({ kind: "registers" });
  });

  it("clicking a harvard cache box while active opens the cache view with the correct side", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const hierarchyStore = useHierarchyConfigStore();
    hierarchyStore.hierarchy = [
      {
        id: "l1",
        harvard: true,
        i: { exp: 6, config: { blockSize: 64, associativity: 2, replacement: "LRU", writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
        d: { exp: 6, config: { blockSize: 64, associativity: 2, replacement: "LRU", writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
        inclusion: "NonInclusive",
        busWidth: 32,
      },
    ] as unknown as typeof hierarchyStore.hierarchy;

    const { container } = render(App);
    const dBox = container.querySelector('[data-box-id="l1-d"]');
    expect(dBox).toBeTruthy();
    await (dBox as HTMLElement).dispatchEvent(new Event("click", { bubbles: true }));
    expect(store.activeView).toEqual({ kind: "cache", levelId: "l1", side: "d" });
  });

  it("clicking the RAM box while active opens the ram view (VM disabled by default)", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const { container } = render(App);
    const ramBox = container.querySelector('[data-box-id="ram"]');
    expect(ramBox).toBeTruthy();
    await (ramBox as HTMLElement).dispatchEvent(new Event("click", { bubbles: true }));
    expect(store.activeView).toEqual({ kind: "ram" });
  });

  it("clicking the PT sub-block while active and VM enabled opens the page table view", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const hierarchyStore = useHierarchyConfigStore();
    hierarchyStore.vmEnabled = true;

    const { container } = render(App);
    await Promise.resolve();
    await Promise.resolve();
    const ptBox = container.querySelector('[data-box-id="ram-pt"]');
    expect(ptBox).toBeTruthy();
    await (ptBox as HTMLElement).dispatchEvent(new Event("click", { bubbles: true }));
    expect(store.activeView).toEqual({ kind: "pageTable" });
  });

  it("clicking the TLB box while active and VM enabled opens the TLB view", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const hierarchyStore = useHierarchyConfigStore();
    hierarchyStore.vmEnabled = true;

    const { container } = render(App);
    await Promise.resolve();
    await Promise.resolve();
    const tlbBox = container.querySelector('[data-box-id="tlb"]');
    expect(tlbBox).toBeTruthy();
    await (tlbBox as HTMLElement).dispatchEvent(new Event("click", { bubbles: true }));
    expect(store.activeView).toEqual({ kind: "tlb" });
  });

  it("triggers a hit pulse on the matching cache box for a cache_access event", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const hierarchyStore = useHierarchyConfigStore();
    hierarchyStore.hierarchy = [
      {
        id: "l1",
        harvard: false,
        unified: { exp: 1, config: { blockSize: 4, associativity: 1, replacement: "LRU", writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
        inclusion: "NonInclusive",
        busWidth: 32,
      },
    ] as unknown as typeof hierarchyStore.hierarchy;

    render(App);
    await Promise.resolve();
    await Promise.resolve();

    const box = document.querySelector('[data-box-id="l1"]');
    expect(box).toBeTruthy();

    const event = { payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1 } };
    // Call all captured handlers; only the one wired to the diagram will actually change anything visible
    for (const handler of allHandlers) {
      handler(event);
    }

    // The screen doesn't update instantly, so wait before checking the class
    // Note: SVG elements need getAttribute here instead of the usual className check
    await waitFor(() => {
      expect(box!.getAttribute("class")).toContain("access-hit");
    });
  });

  it("clears the last hit/miss colors and status message once the simulation is no longer active", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const hierarchyStore = useHierarchyConfigStore();
    hierarchyStore.hierarchy = [
      {
        id: "l1",
        harvard: false,
        unified: { exp: 1, config: { blockSize: 4, associativity: 1, replacement: "LRU", writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
        inclusion: "NonInclusive",
        busWidth: 32,
      },
    ] as unknown as typeof hierarchyStore.hierarchy;

    render(App);
    await Promise.resolve();
    await Promise.resolve();

    const box = document.querySelector('[data-box-id="l1"]');
    const event = { payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1, accessKind: "load", address: 0x1000 } };
    for (const handler of allHandlers) handler(event);

    await waitFor(() => {
      expect(box!.getAttribute("class")).toContain("access-hit");
      expect(document.querySelector(".hierarchy-diagram-status")).toBeTruthy();
    });

    // The simulation ends, whether Stop was pressed or it just stopped being
    // active, so no further sim event will arrive to trigger the usual
    // before-the-next-step clear. This must reset on its own instead.
    store.isActive = false;

    await waitFor(() => {
      expect(box!.getAttribute("class")).not.toContain("access-hit");
      expect(document.querySelector(".hierarchy-diagram-status")).toBeNull();
    });
  });
});
