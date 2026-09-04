import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
let capturedHandlers: Array<(event: { payload: Record<string, unknown> }) => void> = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_channel: string, handler: (event: { payload: Record<string, unknown> }) => void) => {
    capturedHandlers.push(handler);
    return Promise.resolve(() => {});
  }),
}));

import SimulationInfoPanel from "./SimulationInfoPanel.vue";
import { useSimulationStore } from "../../stores/simulation";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

beforeEach(() => {
  setActivePinia(createPinia());
  capturedHandlers = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "get_cache_level_info") return Promise.resolve({ associativity: 1, blockSize: 4, numSets: 1, wordsPerLine: 1 });
    if (cmd === "get_cache_sets") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
});

function seedTwoLevels() {
  const store = useHierarchyConfigStore();
  const makeLevel = (id: string) => ({
    id,
    harvard: false,
    unified: { exp: 6, config: { blockSize: 64, associativity: 4, replacement: "LRU", writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
    inclusion: "NonInclusive",
    busWidth: 64,
  });
  store.hierarchy = [makeLevel("l1"), makeLevel("l2")] as unknown as typeof store.hierarchy;
  return store;
}

describe("SimulationInfoPanel", () => {
  it("hides the Stats button while already on the stats view", () => {
    render(SimulationInfoPanel);
    expect(screen.queryByLabelText("Stats")).toBeNull();
  });

  it("shows the Stats button once a non-stats view is active", () => {
    const store = useSimulationStore();
    store.openView({ kind: "registers" });
    render(SimulationInfoPanel);
    expect(screen.getByLabelText("Stats")).toBeTruthy();
  });

  it("defaults to the stats view", () => {
    const { container } = render(SimulationInfoPanel);
    expect(container.querySelector(".simulation-stats-view")).toBeTruthy();
  });

  it("switches to the registers view when activeView is registers", () => {
    const store = useSimulationStore();
    store.openView({ kind: "registers" });
    const { container } = render(SimulationInfoPanel);
    expect(container.querySelector(".simulation-registers-view")).toBeTruthy();
    expect(container.querySelector(".simulation-stats-view")).toBeFalsy();
  });

  it("fetches a fresh snapshot when switching between two cache levels", async () => {
    seedTwoLevels();
    const store = useSimulationStore();
    store.openView({ kind: "cache", levelId: "l1", side: "unified" });
    render(SimulationInfoPanel);
    await Promise.resolve();
    await Promise.resolve();

    expect(invokeMock).toHaveBeenCalledWith("get_cache_level_info", { levelIndex: 0, side: "unified" });
    invokeMock.mockClear();

    store.openView({ kind: "cache", levelId: "l2", side: "unified" });
    await Promise.resolve();
    await Promise.resolve();

    expect(invokeMock).toHaveBeenCalledWith("get_cache_level_info", { levelIndex: 1, side: "unified" });
  });
});
