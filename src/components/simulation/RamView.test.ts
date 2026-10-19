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

import RamView from "./RamView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useCompileConfigStore } from "../../stores/compileConfig";

// Blocks are cached by their actual row (address divided by 32), unlike the
// old scheme that was relative to the visible window. So the fake response
// here must return an address that matches the requested startAddress, or
// the row on screen won't find it.
function mockWordAtRequestedAddress(word: number) {
  invokeMock.mockImplementation((_cmd: string, args: { startAddress: number }) =>
    Promise.resolve([{ address: args.startAddress, words: [word], populated: true }]),
  );
}

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  mockWordAtRequestedAddress(0x12345678);
  capturedHandler = null;
  useHierarchyConfigStore().ram = { latency: 100, size: 1024, dirtyPercent: 0, pageTableLevels: 1, frameAllocation: "sequential", frames: 4 };
  useCompileConfigStore().dataBase = 0x10000000;
});

// Jumping to an address centers it in the visible window, so the range
// fetched starts a little before it rather than exactly at it. This checks
// that the fetched range actually covers the target address, instead of
// expecting an exact start.
function expectLastFetchCovers(address: number) {
  const [, args] = invokeMock.mock.calls.at(-1) as [string, { startAddress: number; count: number }];
  expect(args.startAddress).toBeLessThanOrEqual(address);
  expect(args.startAddress + args.count * 32).toBeGreaterThan(address);
}

describe("RamView", () => {
  it("fetches a window covering .text on mount, centered so there's room to scroll either direction", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    expectLastFetchCovers(0x00400000);
    expect(await screen.findByText("0x12345678")).toBeTruthy();
  });

  it("shows a dash for an unpopulated cell", async () => {
    invokeMock.mockImplementation((_cmd: string, args: { startAddress: number }) =>
      Promise.resolve([{ address: args.startAddress, words: [0], populated: false, dirty: false }]),
    );
    const { container } = render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    expect(await screen.findByText("-")).toBeTruthy();
    expect(screen.queryByText("0x00000000")).toBeNull();
    expect(container.querySelector(".simulation-ram-cell-empty")).toBeTruthy();
  });

  it("also shows a dash — not the garbage value — for a dirty-seeded cell the program hasn't actually touched yet", async () => {
    // Garbage data placed there ahead of time should only become visible
    // once the simulated program actually reads or writes that address.
    // Just looking at the memory (what this fake response simulates) must
    // never reveal it, so the display treats it exactly the same as any
    // other empty cell.
    invokeMock.mockImplementation((_cmd: string, args: { startAddress: number }) =>
      Promise.resolve([{ address: args.startAddress, words: [0xdeadbeef], populated: false, dirty: true }]),
    );
    const { container } = render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    expect(await screen.findByText("-")).toBeTruthy();
    expect(screen.queryByText("0xDEADBEEF")).toBeNull();
    expect(container.querySelector(".simulation-ram-cell-empty")).toBeTruthy();
  });

  it("zero-pads a small word value to the full 4-byte width", async () => {
    mockWordAtRequestedAddress(0xf);
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    expect(await screen.findByText("0x0000000F")).toBeTruthy();
    expect(screen.queryByText("0xF")).toBeNull();
  });

  it("clicking .data jumps the window to dataBase", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    await fireEvent.click(screen.getByText(".data"));
    await Promise.resolve();
    await Promise.resolve();

    expectLastFetchCovers(0x10000000);
  });

  it("clicking .text jumps the window back to textBase", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    await fireEvent.click(screen.getByText(".data"));
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    await fireEvent.click(screen.getByText(".text"));
    await Promise.resolve();
    await Promise.resolve();

    expectLastFetchCovers(0x00400000);
  });

  it("jumps to a typed hex address on Enter", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    const input = screen.getByLabelText("Jump to address");
    await fireEvent.update(input, "0x20000000");
    await fireEvent.keyDown(input, { key: "Enter" });
    await Promise.resolve();
    await Promise.resolve();

    expectLastFetchCovers(0x20000000);
  });

  it("jumps to a typed hex address via the Go button", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    await fireEvent.update(screen.getByLabelText("Jump to address"), "0x300");
    await fireEvent.click(screen.getByText("Go"));
    await Promise.resolve();
    await Promise.resolve();

    expectLastFetchCovers(0x300);
  });

  it("does not jump on an invalid (non-hex) address and marks the field invalid", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    const input = screen.getByLabelText("Jump to address");
    await fireEvent.update(input, "not-hex");
    await fireEvent.click(screen.getByText("Go"));
    await Promise.resolve();

    expect(invokeMock).not.toHaveBeenCalled();
    expect(input.className).toContain("simulation-ram-jump-input-invalid");
  });

  it("re-anchors near the bottom edge, so scrolling isn't capped at ram.size like it used to be", async () => {
    const { container } = render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    const scrollEl = container.querySelector(".simulation-ram-body") as HTMLDivElement;
    invokeMock.mockClear();

    // Scroll to near the bottom of the visible window.
    scrollEl.scrollTop = 1000 * 26 - 100;
    await fireEvent.scroll(scrollEl);
    await Promise.resolve();
    await Promise.resolve();

    const [, args] = invokeMock.mock.calls.at(-1) as [string, { startAddress: number }];
    // This is far beyond textBase plus the old 1024-byte RAM size limit,
    // which proves scrolling is no longer stuck at a small fixed window.
    expect(args.startAddress).toBeGreaterThan(0x00400000 + 1024);
  });

  it("swallows the synthetic scroll event its own re-anchoring causes, instead of feeding back into another recenter", async () => {
    const { container } = render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    const scrollEl = container.querySelector(".simulation-ram-body") as HTMLDivElement;

    // This triggers a recenter, which sets the scroll position in code. In
    // a real browser, setting the scroll position like that fires its own
    // scroll event.
    scrollEl.scrollTop = 1000 * 26 - 100;
    await fireEvent.scroll(scrollEl);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    // Simulate that browser-triggered scroll event. It must be ignored, not
    // treated as a real user scroll, or the window could re-center again
    // and jitter back and forth.
    await fireEvent.scroll(scrollEl);
    await Promise.resolve();
    expect(invokeMock).not.toHaveBeenCalled();

    // A real scroll that happens afterward should still work as normal.
    scrollEl.scrollTop += 100;
    await fireEvent.scroll(scrollEl);
    await Promise.resolve();
    await Promise.resolve();
    expect(invokeMock).toHaveBeenCalled();
  });

  it("snaps a fractional scrollTop to a whole pixel once scrolling settles", async () => {
    vi.useFakeTimers();
    try {
      const { container } = render(RamView);
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(0);
      const scrollEl = container.querySelector(".simulation-ram-body") as HTMLDivElement;

      // A small, realistic scroll move from the centered starting position.
      // It's not near either edge of the window, so it won't also trigger
      // a recenter, which would move the scroll position itself.
      scrollEl.scrollTop = 13000.6;
      await fireEvent.scroll(scrollEl);
      expect(scrollEl.scrollTop).toBe(13000.6);

      await vi.advanceTimersByTimeAsync(100);
      expect(scrollEl.scrollTop).toBe(13000);
    } finally {
      vi.useRealTimers();
    }
  });

  it("refetches the visible window on a ram_access event", async () => {
    render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "ram_access", outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();
    await Promise.resolve();

    expectLastFetchCovers(0x00400000);
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

describe("RamView — address digit width", () => {
  it("pads the address column to 8 hex digits on RV32I", async () => {
    mockWordAtRequestedAddress(0x1);
    const { container } = render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    const address = container.querySelector(".simulation-ram-address") as HTMLElement;
    expect(address.textContent).toMatch(/^0x[0-9A-F]{8}$/);
  });

  it("pads the address column to 16 hex digits on RV64I", async () => {
    useHierarchyConfigStore().isa = "RV64I";
    mockWordAtRequestedAddress(0x1);
    const { container } = render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    const address = container.querySelector(".simulation-ram-address") as HTMLElement;
    expect(address.textContent).toMatch(/^0x[0-9A-F]{16}$/);
  });
});
