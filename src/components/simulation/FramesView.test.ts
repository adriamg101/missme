import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, within } from "@testing-library/vue";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_channel: string, handler: typeof capturedHandler) => {
    capturedHandler = handler;
    return Promise.resolve(() => {});
  }),
}));

import FramesView from "./FramesView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

function seedVmStore() {
  const store = useHierarchyConfigStore();
  store.vmEnabled = true;
  store.pageSize = 4;
  store.ram.frames = 2;
  return store;
}

interface FrameLineRow {
  frame: number;
  subrowIndex: number;
  vpn: number | null;
  valid: boolean;
  dirty: boolean;
  words: number[];
  populated: boolean[];
}

function mockFrameLines(rows: FrameLineRow[]) {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "get_frame_lines") {
      const startLine = args.startLine as number;
      const count = args.count as number;
      return Promise.resolve(rows.slice(startLine, startLine + count));
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

describe("FramesView", () => {
  it("fetches the visible window of frame lines on mount", async () => {
    seedVmStore();
    mockFrameLines([
      { frame: 0, subrowIndex: 0, vpn: 3, valid: true, dirty: false, words: [0xdeadbeef], populated: [true] },
      { frame: 1, subrowIndex: 0, vpn: null, valid: false, dirty: false, words: [], populated: [] },
    ]);
    render(FramesView);
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("get_frame_lines", expect.objectContaining({ startLine: 0 }));
    expect(await screen.findByText("0xDEADBEEF")).toBeTruthy();
  });

  it("shows a dash for an invalid frame", async () => {
    seedVmStore();
    mockFrameLines([{ frame: 0, subrowIndex: 0, vpn: null, valid: false, dirty: false, words: [], populated: [] }]);
    const { container } = render(FramesView);
    await flush();
    expect(container.querySelectorAll(".simulation-frames-cell-empty").length).toBeGreaterThan(0);
  });

  it("shows a dash for a word inside a valid frame that was never actually written", async () => {
    seedVmStore();
    mockFrameLines([{ frame: 0, subrowIndex: 0, vpn: 1, valid: true, dirty: false, words: [0xdeadbeef], populated: [false] }]);
    const { container } = render(FramesView);
    await flush();
    expect(screen.queryByText("0xDEADBEEF")).toBeNull();
    expect(container.querySelectorAll(".simulation-frames-cell-empty").length).toBeGreaterThan(0);
  });

  it("splits a page bigger than 8 words into multiple 8-word sub-rows with an Offset column", async () => {
    const store = seedVmStore();
    store.pageSize = 64; // 16 words -> 2 sub-rows of 8
    mockFrameLines([
      { frame: 0, subrowIndex: 0, vpn: 1, valid: true, dirty: false, words: Array.from({ length: 8 }, (_, i) => i), populated: Array(8).fill(true) },
      { frame: 0, subrowIndex: 1, vpn: 1, valid: true, dirty: false, words: Array.from({ length: 8 }, (_, i) => i + 8), populated: Array(8).fill(true) },
    ]);
    const { container } = render(FramesView);
    await flush();

    expect(await screen.findByText("Offset")).toBeTruthy();
    const body = within(container.querySelector(".simulation-frames-body")!);
    expect(await body.findByText("0x00")).toBeTruthy();
    expect(await body.findByText("0x20")).toBeTruthy();
  });

  it("jumping to a frame number requests a window that includes that frame's first line", async () => {
    const store = seedVmStore();
    store.pageSize = 4;
    store.ram.frames = 100_000; // far more than one screenful
    mockFrameLines([{ frame: 0, subrowIndex: 0, vpn: null, valid: false, dirty: false, words: [], populated: [] }]);
    const { getByLabelText, getByTitle } = render(FramesView);
    await flush();
    invokeMock.mockClear();

    const input = getByLabelText("Jump to frame") as HTMLInputElement;
    input.value = "60000";
    input.dispatchEvent(new Event("input"));
    getByTitle("Jump to this frame").dispatchEvent(new Event("click"));
    await flush();

    const call = invokeMock.mock.calls.find((c) => c[0] === "get_frame_lines");
    expect(call).toBeTruthy();
    const args = call![1] as { startLine: number; count: number };
    expect(args.startLine).toBeGreaterThan(0);
    expect(args.startLine).toBeLessThanOrEqual(60000);
    expect(args.startLine + args.count).toBeGreaterThan(60000 - args.count);
  });

  it("refetches on a ram_access event", async () => {
    seedVmStore();
    mockFrameLines([{ frame: 0, subrowIndex: 0, vpn: null, valid: false, dirty: false, words: [], populated: [] }]);
    render(FramesView);
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "ram_access", outcome: "hit", sourceLine: 1 } });
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_frame_lines", expect.anything());
  });

  it("requests a bounded window even when a page is far wider than one call's cap", async () => {
    const store = seedVmStore();
    store.pageSize = 4096; // 1024 words, 128 subrows, far more than the old per-frame fetch implied
    store.ram.frames = 1;
    mockFrameLines(
      Array.from({ length: 128 }, (_, i) => ({ frame: 0, subrowIndex: i, vpn: null, valid: false, dirty: false, words: [], populated: [] })),
    );
    render(FramesView);
    await flush();

    for (const call of invokeMock.mock.calls) {
      if (call[0] === "get_frame_lines") {
        expect((call[1] as { count: number }).count).toBeLessThanOrEqual(512);
      }
    }
  });
});
