import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_channel: string, handler: typeof capturedHandler) => {
    capturedHandler = handler;
    return Promise.resolve(() => {});
  }),
}));

import CacheLevelView from "./CacheLevelView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

beforeEach(() => {
  setActivePinia(createPinia());
  capturedHandler = null;
});

function seedOneLevel() {
  const store = useHierarchyConfigStore();
  store.hierarchy = [
    {
      id: "l1",
      harvard: false,
      unified: { exp: 6, config: { blockSize: 64, associativity: 4, replacement: "LRU", writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
      inclusion: "NonInclusive",
      busWidth: 64,
    },
  ] as unknown as typeof store.hierarchy;
  return store;
}

describe("CacheLevelView", () => {
  it("shows the resolved level's static config", () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1" } });
    expect(screen.getByText(/Associativitat/)).toBeTruthy();
  });

  it("counts hit/miss events for the matching level index", async () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1" } });
    await Promise.resolve();
    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1 } });
    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 0, outcome: "miss", sourceLine: 1 } });
    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 1, outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();
    expect(screen.getByText("1", { selector: ".simulation-cache-hits" })).toBeTruthy();
    expect(screen.getByText("1", { selector: ".simulation-cache-misses" })).toBeTruthy();
  });
});
