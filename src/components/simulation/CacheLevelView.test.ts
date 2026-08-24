import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_channel: string, handler: typeof capturedHandler) => {
    capturedHandler = handler;
    return Promise.resolve(() => {});
  }),
}));

import CacheLevelView from "./CacheLevelView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

function seedOneLevel() {
  const store = useHierarchyConfigStore();
  store.hierarchy = [
    {
      id: "l1",
      harvard: false,
      unified: { exp: 1, config: { blockSize: 4, associativity: 1, replacement: "LRU", writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
      inclusion: "NonInclusive",
      busWidth: 32,
    },
  ] as unknown as typeof store.hierarchy;
  return store;
}

function snapshot() {
  return {
    associativity: 1,
    blockSize: 4,
    numSets: 2,
    wordsPerLine: 1,
    sets: [
      { setIndex: 0, ways: [{ valid: true, dirty: false, tag: 5, recencyRank: 0, words: [0xdeadbeef] }] },
      { setIndex: 1, ways: [{ valid: false, dirty: false, tag: 0, recencyRank: null, words: [] }] },
    ],
  };
}

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(snapshot());
  capturedHandler = null;
});

describe("CacheLevelView", () => {
  it("fetches and renders the level snapshot on mount", async () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await Promise.resolve();
    await Promise.resolve();
    expect(invokeMock).toHaveBeenCalledWith("get_cache_level", { levelIndex: 0, side: "unified" });
    expect(await screen.findByText("deadbeef")).toBeTruthy();
  });

  it("refetches the level on a matching cache_access event", async () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();
    await Promise.resolve();

    expect(invokeMock).toHaveBeenCalledWith("get_cache_level", { levelIndex: 0, side: "unified" });
  });

  it("ignores a cache_access event for a different level", async () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 1, outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();

    expect(invokeMock).not.toHaveBeenCalled();
  });
});
