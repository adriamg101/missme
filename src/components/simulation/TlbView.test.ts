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

import TlbView from "./TlbView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

interface TlbEntry {
  valid: boolean;
  vpn: number;
  ppn: number;
  orderValue: number | null;
}
interface TlbSet {
  setIndex: number;
  entries: TlbEntry[];
}

function mockTlb(info: { associativity: number; numSets: number }, allSets: TlbSet[]) {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "get_tlb_info") return Promise.resolve(info);
    if (cmd === "get_tlb_sets") {
      const startSet = args.startSet as number;
      const count = args.count as number;
      return Promise.resolve(allSets.filter((s) => s.setIndex >= startSet && s.setIndex < startSet + count));
    }
    return Promise.resolve(undefined);
  });
}

async function flush() {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  capturedHandler = null;
});

describe("TlbView", () => {
  it("fetches TLB info then the visible window of sets on mount", async () => {
    mockTlb({ associativity: 1, numSets: 2 }, [
      { setIndex: 0, entries: [{ valid: true, vpn: 5, ppn: 9, orderValue: 0 }] },
      { setIndex: 1, entries: [{ valid: false, vpn: 0, ppn: 0, orderValue: null }] },
    ]);
    render(TlbView);
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("get_tlb_info", {});
    expect(await screen.findByText("0x00005")).toBeTruthy();
    expect(await screen.findByText("0x9")).toBeTruthy();
  });

  it("shows a dash for an invalid way", async () => {
    mockTlb({ associativity: 1, numSets: 1 }, [{ setIndex: 0, entries: [{ valid: false, vpn: 0, ppn: 0, orderValue: null }] }]);
    const { container } = render(TlbView);
    await flush();
    const dashes = container.querySelectorAll(".simulation-tlb-cell-empty");
    expect(dashes.length).toBeGreaterThan(0);
  });

  it("refetches the visible window on a tlb_access event", async () => {
    mockTlb({ associativity: 1, numSets: 1 }, [{ setIndex: 0, entries: [{ valid: false, vpn: 0, ppn: 0, orderValue: null }] }]);
    render(TlbView);
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "tlb_access", outcome: "hit", sourceLine: 1 } });
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_tlb_sets", expect.objectContaining({ startSet: 0 }));
    expect(invokeMock).not.toHaveBeenCalledWith("get_tlb_info", expect.anything());
  });

  it("ignores unrelated sim events", async () => {
    mockTlb({ associativity: 1, numSets: 1 }, [{ setIndex: 0, entries: [{ valid: false, vpn: 0, ppn: 0, orderValue: null }] }]);
    render(TlbView);
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();

    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("TlbView — replacement policy column", () => {
  it("labels the order column after the active policy (FIFO)", async () => {
    const store = useHierarchyConfigStore();
    store.tlb.replacement = "FIFO";
    mockTlb({ associativity: 1, numSets: 1 }, [{ setIndex: 0, entries: [{ valid: true, vpn: 1, ppn: 2, orderValue: 0 }] }]);
    render(TlbView);
    await flush();

    expect(await screen.findByText("FIFO")).toBeTruthy();
    expect(screen.queryByText("LRU")).toBeNull();
  });

  it("hides the order column entirely when the policy is Random", async () => {
    const store = useHierarchyConfigStore();
    store.tlb.replacement = "Random";
    mockTlb({ associativity: 1, numSets: 1 }, [{ setIndex: 0, entries: [{ valid: true, vpn: 1, ppn: 2, orderValue: null }] }]);
    const { container } = render(TlbView);
    await flush();

    expect(screen.queryByText("LRU")).toBeNull();
    expect(screen.queryByText("FIFO")).toBeNull();
    expect(await screen.findByText("0x00001")).toBeTruthy();
    const headerCells = container.querySelectorAll(".simulation-tlb-grid-header .simulation-tlb-cell");
    expect(headerCells.length).toBe(4); // Set, V, VPN, PPN - one fewer than the default 5
  });
});
