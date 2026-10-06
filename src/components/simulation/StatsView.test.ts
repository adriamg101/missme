import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_channel: string, handler: typeof capturedHandler) => {
    capturedHandler = handler;
    return Promise.resolve(() => {});
  }),
}));

import StatsView from "./StatsView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

function emptyStats() {
  return {
    instructionsCompleted: 3,
    totalMemoryAccesses: 5,
    topOfChainInstructionAccesses: 3,
    topOfChainDataAccesses: 2,
    totalCycles: 20,
    amat: 4,
    cpi: 6.67,
    buckets: [
      { bucketKind: "ram", levelIndex: null, side: null, accesses: 5, hits: 0, misses: 0, cycles: 20 },
    ],
  };
}

async function flush() {
  for (let i = 0; i < 4; i++) await Promise.resolve();
}

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(emptyStats());
  capturedHandler = null;
});

describe("StatsView", () => {
  it("always shows a Global tab first, selected by default", async () => {
    render(StatsView);
    await flush();
    const tabs = screen.getAllByRole("tab").map((t) => t.textContent);
    expect(tabs[0]).toBe("Global");
    expect(screen.getByRole("tab", { name: "Global" })).toHaveAttribute("aria-selected", "true");
  });

  it("shows the TLB and disk tabs only when virtual memory is on", async () => {
    const store = useHierarchyConfigStore();
    store.vmEnabled = true;
    render(StatsView);
    await flush();
    expect(screen.getByRole("tab", { name: "TLB" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "DISK" })).toBeTruthy();
  });

  it("hides the TLB and disk tabs when virtual memory is off", async () => {
    render(StatsView);
    await flush();
    expect(screen.queryByRole("tab", { name: "TLB" })).toBeNull();
    expect(screen.queryByRole("tab", { name: "DISK" })).toBeNull();
  });

  it("fetches stats on mount and refetches on a matching sim:event", async () => {
    render(StatsView);
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("get_stats", {});
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "ram_access", outcome: "hit", address: 0, accessKind: "load", sourceLine: 1 } });
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_stats", {});
  });

  it("shows the RAM tab's page fault cells only when virtual memory is on", async () => {
    const store = useHierarchyConfigStore();
    store.vmEnabled = true;
    render(StatsView);
    await flush();
    await fireEvent.click(screen.getByRole("tab", { name: "RAM" }));
    expect(screen.getByText("Page hits")).toBeTruthy();
    expect(screen.getByText("Page faults")).toBeTruthy();
  });
});
