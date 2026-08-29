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

// Blocks are cached by absolute row (address / 32), so — unlike the old
// window-relative scheme — the mock must return an address that actually
// matches the requested startAddress for the rendered row to find it.
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
  useHierarchyConfigStore().ram = { latency: 100, size: 1024, dirtyPercent: 0 };
  useCompileConfigStore().dataBase = 0x10000000;
});

// A jump centers the target address in the window, so the fetched range
// starts a bit before it (overscan) rather than exactly at it — this
// checks the fetched window actually covers the target instead of
// asserting an exact startAddress.
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
    // dirty-seeded garbage only becomes visible once the simulated program
    // really reads/writes that address — a passive inspection peek (this
    // mock) must never leak it, so the display treats it exactly like any
    // other unpopulated cell.
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

    // Scroll to near the bottom of the sliding window (WINDOW_ROWS * ROW_HEIGHT).
    scrollEl.scrollTop = 20_000 * 26 - 100;
    await fireEvent.scroll(scrollEl);
    await Promise.resolve();
    await Promise.resolve();

    const [, args] = invokeMock.mock.calls.at(-1) as [string, { startAddress: number }];
    // Far beyond textBase + the old 1024-byte ram.size cap — proves this
    // no longer dead-ends at a fixed small window.
    expect(args.startAddress).toBeGreaterThan(0x00400000 + 1024);
  });

  it("swallows the synthetic scroll event its own re-anchoring causes, instead of feeding back into another recenter", async () => {
    const { container } = render(RamView);
    await Promise.resolve();
    await Promise.resolve();
    const scrollEl = container.querySelector(".simulation-ram-body") as HTMLDivElement;

    // Triggers a recenter, which programmatically writes scrollEl.scrollTop
    // — in a real browser that write itself dispatches a 'scroll' event.
    scrollEl.scrollTop = 20_000 * 26 - 100;
    await fireEvent.scroll(scrollEl);
    await Promise.resolve();
    await Promise.resolve();
    invokeMock.mockClear();

    // Simulate that browser-dispatched synthetic event: it must be
    // swallowed, not treated as a real user scroll (which would re-run
    // maybeRecenter and could jitter the window back and forth).
    await fireEvent.scroll(scrollEl);
    await Promise.resolve();
    expect(invokeMock).not.toHaveBeenCalled();

    // A genuine subsequent scroll still works normally afterwards.
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

      // A small, realistic scroll delta from the centered mount position
      // (260000px) — not near either edge of the window, so this doesn't
      // also trigger a recenter, which would itself move scrollTop.
      scrollEl.scrollTop = 260000.6;
      await fireEvent.scroll(scrollEl);
      expect(scrollEl.scrollTop).toBe(260000.6);

      await vi.advanceTimersByTimeAsync(100);
      expect(scrollEl.scrollTop).toBe(260000);
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
