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

import RamView from "./RamView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useCompileConfigStore } from "../../stores/compileConfig";

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([{ address: 0, words: [0x12345678], populated: true }]);
  capturedHandler = null;
  useHierarchyConfigStore().ram = { latency: 100, size: 1024, dirtyPercent: 0 };
  useCompileConfigStore().dataBase = 0x10000000;
});

describe("RamView", () => {
  it("fetches the visible window on mount", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    expect(invokeMock).toHaveBeenCalledWith("get_ram_blocks", expect.objectContaining({ startAddress: 0x10000000 }));
    expect(await screen.findByText("12345678")).toBeTruthy();
  });

  it("refetches the visible window on a ram_access event", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "ram_access", outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();
    await Promise.resolve();

    expect(invokeMock).toHaveBeenCalledWith("get_ram_blocks", expect.objectContaining({ startAddress: 0x10000000 }));
  });

  it("ignores non-ram_access events", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();

    expect(invokeMock).not.toHaveBeenCalled();
  });
});
