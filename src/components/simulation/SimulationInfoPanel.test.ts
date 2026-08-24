import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
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
  it("shows the Stats button", () => {
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

  it("resets hit/miss counts when switching between two cache levels", async () => {
    seedTwoLevels();
    const store = useSimulationStore();
    store.openView({ kind: "cache", levelId: "l1" });
    render(SimulationInfoPanel);
    await Promise.resolve();

    expect(capturedHandlers.length).toBe(1);
    capturedHandlers[0]({ payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();
    expect(screen.getByText("1", { selector: ".simulation-cache-hits" })).toBeTruthy();

    store.openView({ kind: "cache", levelId: "l2" });
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.getByText("0", { selector: ".simulation-cache-hits" })).toBeTruthy();
  });
});
