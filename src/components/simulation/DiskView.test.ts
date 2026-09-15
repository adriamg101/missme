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

import DiskView from "./DiskView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useCompileConfigStore } from "../../stores/compileConfig";

interface DiskPage {
  address: number; // vpn, reusing the RamBlockSnapshot shape
  words: number[];
  populated: boolean;
  dirty: boolean;
}

function mockDisk(pages: DiskPage[]) {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "get_disk_pages") {
      const startVpn = args.startVpn as number;
      const count = args.count as number;
      return Promise.resolve(pages.filter((p) => p.address >= startVpn && p.address < startVpn + count));
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

describe("DiskView", () => {
  it("fetches the visible window of blocks on mount, addressed like RAM (block 0 = VPN 0)", async () => {
    const store = useHierarchyConfigStore();
    store.pageSize = 32; // 1 block per page, so VPN 0 covers address 0x0-0x1F
    mockDisk([{ address: 0, words: [0x11111111], populated: true, dirty: false }]);
    const { container } = render(DiskView);
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("get_disk_pages", expect.objectContaining({ startVpn: 0 }));
    expect(await screen.findByText("0x11111111")).toBeTruthy();
    const addressCell = container.querySelector(".simulation-disk-address");
    expect(addressCell?.textContent).toBe("0x00000000"); // the block's own address, shown like RamView's Address column
    const pageCell = container.querySelector(".simulation-disk-vpn");
    expect(pageCell?.textContent).toBe("0x0000000"); // VPN 0, since pageSize (32) == BLOCK_SIZE here
  });

  it("groups consecutive blocks from the same page under one Page cell", async () => {
    const store = useHierarchyConfigStore();
    store.pageSize = 64; // 2 blocks per page
    mockDisk([{ address: 0, words: Array.from({ length: 16 }, (_, i) => i), populated: true, dirty: false }]);
    const { container } = render(DiskView);
    await flush();

    // Both 32-byte blocks of VPN 0 must share a single Page cell (spanning
    // 2 rows), not one Page cell per block. The visible window covers more
    // than just VPN 0's blocks (there's no measured viewport in this test,
    // so a few extra rows of overscan render too), so this looks specifically
    // for VPN 0's own cell rather than asserting how many Page cells exist
    // in total.
    const pageCells = Array.from(container.querySelectorAll(".simulation-disk-vpn"));
    const vpn0Cell = pageCells.find((c) => c.textContent === "0x0000000");
    expect(vpn0Cell).toBeTruthy();
    expect((vpn0Cell as HTMLElement).style.gridRow).toBe("1 / span 2");
  });

  it("shows a dash for a block that was never touched", async () => {
    useHierarchyConfigStore().pageSize = 32;
    mockDisk([{ address: 0, words: [0], populated: false, dirty: false }]);
    const { container } = render(DiskView);
    await flush();
    expect(container.querySelectorAll(".simulation-disk-cell-empty").length).toBeGreaterThan(0);
  });

  it("refetches on a disk_access event", async () => {
    useHierarchyConfigStore().pageSize = 32;
    mockDisk([{ address: 0, words: [0], populated: false, dirty: false }]);
    render(DiskView);
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "disk_access", outcome: "hit", sourceLine: 1 } });
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_disk_pages", expect.anything());
  });

  it("shows the low-order byte offsets in the header, same as RamView's word columns", async () => {
    useHierarchyConfigStore().pageSize = 32;
    mockDisk([{ address: 0, words: [0], populated: true, dirty: false }]);
    render(DiskView);
    await flush();
    expect(await screen.findByText("0x00")).toBeTruthy();
    expect(await screen.findByText("0x1C")).toBeTruthy();
  });

  it("a block in the middle of a larger page reads that page's words at the right offset", async () => {
    const store = useHierarchyConfigStore();
    store.pageSize = 64; // 2 blocks per page (32 bytes = 8 words each)
    mockDisk([{ address: 0, words: Array.from({ length: 16 }, (_, i) => i + 1), populated: true, dirty: false }]);
    render(DiskView);
    await flush();
    // Block 1 (the page's second 32-byte block) starts at address 0x20 and
    // should show words[8..16], i.e. the value 9 as its first word.
    expect(await screen.findByText("0x00000009")).toBeTruthy();
  });

  it("jumpToText requests the window around the .text base address", async () => {
    const store = useHierarchyConfigStore();
    store.pageSize = 32;
    mockDisk([{ address: 0, words: [0], populated: false, dirty: false }]);
    const { getByTitle } = render(DiskView);
    await flush();
    invokeMock.mockClear();

    getByTitle("Jump to .text").dispatchEvent(new Event("click"));
    await flush();

    const textBase = useCompileConfigStore().textBase;
    const call = invokeMock.mock.calls.find((c) => c[0] === "get_disk_pages");
    expect(call).toBeTruthy();
    const args = call![1] as { startVpn: number; count: number };
    const targetVpn = Math.floor(textBase / store.pageSize);
    expect(args.startVpn).toBeLessThanOrEqual(targetVpn);
    expect(args.startVpn + args.count).toBeGreaterThan(targetVpn);
  });

  it("jumpToInputPage parses a page number and jumps to it, rejecting invalid input", async () => {
    useHierarchyConfigStore().pageSize = 32;
    mockDisk([{ address: 0, words: [0], populated: false, dirty: false }]);
    const { getByLabelText, getByTitle } = render(DiskView);
    await flush();

    const input = getByLabelText("Jump to page") as HTMLInputElement;
    input.value = "not-a-number";
    input.dispatchEvent(new Event("input"));
    getByTitle("Jump to this page").dispatchEvent(new Event("click"));
    await flush();
    expect(input.className).toContain("simulation-disk-jump-input-invalid");

    invokeMock.mockClear();
    input.value = "1000";
    input.dispatchEvent(new Event("input"));
    getByTitle("Jump to this page").dispatchEvent(new Event("click"));
    await flush();

    const call = invokeMock.mock.calls.find((c) => c[0] === "get_disk_pages");
    expect(call).toBeTruthy();
    const args = call![1] as { startVpn: number; count: number };
    expect(args.startVpn).toBeLessThanOrEqual(1000);
    expect(args.startVpn + args.count).toBeGreaterThan(1000);
  });
});
