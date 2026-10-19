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

interface Frame {
  frame: number;
  vpn: number | null;
  valid: boolean;
  dirty: boolean;
  words: number[];
  populated: boolean[];
}

function mockFrames(frames: Frame[]) {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "get_frames") {
      const startFrame = args.startFrame as number;
      const count = args.count as number;
      return Promise.resolve(frames.filter((f) => f.frame >= startFrame && f.frame < startFrame + count));
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
  it("fetches the visible window of frames on mount", async () => {
    seedVmStore();
    mockFrames([
      { frame: 0, vpn: 3, valid: true, dirty: false, words: [0xdeadbeef], populated: [true] },
      { frame: 1, vpn: null, valid: false, dirty: false, words: [], populated: [] },
    ]);
    render(FramesView);
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("get_frames", expect.objectContaining({ startFrame: 0 }));
    expect(await screen.findByText("0xDEADBEEF")).toBeTruthy();
  });

  it("shows a dash for an invalid frame", async () => {
    seedVmStore();
    mockFrames([{ frame: 0, vpn: null, valid: false, dirty: false, words: [], populated: [] }]);
    const { container } = render(FramesView);
    await flush();
    expect(container.querySelectorAll(".simulation-frames-cell-empty").length).toBeGreaterThan(0);
  });

  it("shows a dash for a word inside a valid frame that was never actually written", async () => {
    seedVmStore();
    mockFrames([{ frame: 0, vpn: 1, valid: true, dirty: false, words: [0xdeadbeef], populated: [false] }]);
    const { container } = render(FramesView);
    await flush();
    // The whole row has "V"/"D" and a dash column for the unpopulated word,
    // so the word's own written value must never appear.
    expect(screen.queryByText("0xDEADBEEF")).toBeNull();
    expect(container.querySelectorAll(".simulation-frames-cell-empty").length).toBeGreaterThan(0);
  });

  it("splits a page bigger than 8 words into multiple 8-word sub-rows with an Offset column", async () => {
    const store = seedVmStore();
    store.pageSize = 64; // 16 words -> 2 sub-rows of 8
    mockFrames([
      { frame: 0, vpn: 1, valid: true, dirty: false, words: Array.from({ length: 16 }, (_, i) => i), populated: Array.from({ length: 16 }, () => true) },
    ]);
    const { container } = render(FramesView);
    await flush();

    expect(await screen.findByText("Offset")).toBeTruthy();
    const body = within(container.querySelector(".simulation-frames-body")!);
    expect(await body.findByText("0x00")).toBeTruthy(); // first sub-row's offset, 2 digits for a 64-byte (6-bit) page
    expect(await body.findByText("0x20")).toBeTruthy(); // second sub-row's offset (8 words * 4 bytes)
  });

  it("jumping to a frame number requests a window that includes that frame", async () => {
    const store = seedVmStore();
    store.pageSize = 4;
    store.ram.frames = 100_000; // far more than one screenful
    mockFrames([{ frame: 0, vpn: null, valid: false, dirty: false, words: [], populated: [] }]);
    const { getByLabelText, getByTitle } = render(FramesView);
    await flush();
    invokeMock.mockClear();

    const input = getByLabelText("Jump to frame") as HTMLInputElement;
    input.value = "60000";
    input.dispatchEvent(new Event("input"));
    getByTitle("Jump to this frame").dispatchEvent(new Event("click"));
    await flush();

    const call = invokeMock.mock.calls.find((c) => c[0] === "get_frames");
    expect(call).toBeTruthy();
    const args = call![1] as { startFrame: number; count: number };
    expect(args.startFrame).toBeGreaterThan(0);
    expect(args.startFrame).toBeLessThanOrEqual(60000);
    expect(args.startFrame + args.count).toBeGreaterThan(60000 - args.count);
  });

  it("refetches on a ram_access event", async () => {
    seedVmStore();
    mockFrames([{ frame: 0, vpn: null, valid: false, dirty: false, words: [], populated: [] }]);
    render(FramesView);
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "ram_access", outcome: "hit", sourceLine: 1 } });
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_frames", expect.anything());
  });
});
