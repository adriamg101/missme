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

import CacheLevelView from "./CacheLevelView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

interface CacheLevelInfo {
  associativity: number;
  blockSize: number;
  numSets: number;
  wordsPerLine: number;
}
interface CacheWay {
  valid: boolean;
  dirty: boolean;
  tag: number;
  recencyRank: number | null;
  words: number[];
}
interface CacheSet {
  setIndex: number;
  ways: CacheWay[];
}

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

// get_cache_level_info returns metadata only; get_cache_sets pages through
// the actual set data — this mocks both, serving get_cache_sets out of the
// given full `allSets` array by slicing whatever [startSet, startSet+count)
// window was requested, the same way the real backend command does.
function mockCacheLevel(info: CacheLevelInfo, allSets: CacheSet[]) {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "get_cache_level_info") return Promise.resolve(info);
    if (cmd === "get_cache_sets") {
      const startSet = args.startSet as number;
      const count = args.count as number;
      return Promise.resolve(allSets.filter((s) => s.setIndex >= startSet && s.setIndex < startSet + count));
    }
    return Promise.resolve(undefined);
  });
}

function defaultInfo(): CacheLevelInfo {
  return { associativity: 1, blockSize: 4, numSets: 2, wordsPerLine: 1 };
}

function defaultSets(): CacheSet[] {
  return [
    { setIndex: 0, ways: [{ valid: true, dirty: false, tag: 5, recencyRank: 0, words: [0xdeadbeef] }] },
    { setIndex: 1, ways: [{ valid: false, dirty: false, tag: 0, recencyRank: null, words: [] }] },
  ];
}

// onMounted's chain is: invoke(get_cache_level_info) -> resetWindow ->
// nextTick (so .simulation-cache-body exists before measuring it) ->
// invoke(get_cache_sets) -> listen(...) — several microtask hops, so a
// fixed small number of `await Promise.resolve()` calls is fragile. This
// just flushes generously.
async function flush() {
  for (let i = 0; i < 8; i++) await Promise.resolve();
}

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  mockCacheLevel(defaultInfo(), defaultSets());
  capturedHandler = null;
});

describe("CacheLevelView", () => {
  it("fetches level info then the visible window of sets on mount", async () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("get_cache_level_info", { levelIndex: 0, side: "unified" });
    expect(invokeMock).toHaveBeenCalledWith(
      "get_cache_sets",
      expect.objectContaining({ levelIndex: 0, side: "unified", startSet: 0 }),
    );
    expect(await screen.findByText("0xDEADBEEF")).toBeTruthy();
    // Tag is zero-padded to 8 hex digits, same as words.
    expect(await screen.findByText("0x00000005")).toBeTruthy();
  });

  it("refetches only the visible window (not level info) on a matching cache_access event", async () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 0, outcome: "hit", sourceLine: 1 } });
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_cache_sets", expect.objectContaining({ levelIndex: 0, side: "unified" }));
    expect(invokeMock).not.toHaveBeenCalledWith("get_cache_level_info", expect.anything());
  });

  it("ignores a cache_access event for a different level", async () => {
    seedOneLevel();
    render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "cache_access", levelIndex: 1, outcome: "hit", sourceLine: 1 } });
    await Promise.resolve();

    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("CacheLevelView — scroll settling", () => {
  it("snaps a fractional scrollTop to a whole pixel once scrolling settles", async () => {
    vi.useFakeTimers();
    try {
      seedOneLevel();
      const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(0);
      const scrollEl = container.querySelector(".simulation-cache-body") as HTMLDivElement;

      scrollEl.scrollTop = 40.6;
      await fireEvent.scroll(scrollEl);
      expect(scrollEl.scrollTop).toBe(40.6);

      await vi.advanceTimersByTimeAsync(100);
      expect(scrollEl.scrollTop).toBe(40);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("CacheLevelView — empty and dirty-seeded ways", () => {
  it("shows a dash for an invalid way instead of a blank cell", async () => {
    seedOneLevel();
    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    // Set 1 in defaultSets() is invalid — its cells must show "-", not
    // blank, and carry the empty-cell class.
    const dashes = container.querySelectorAll(".simulation-cache-cell-empty");
    expect(dashes.length).toBeGreaterThan(0);
    expect(Array.from(dashes).every((cell) => cell.textContent === "-")).toBe(true);
  });

  it("shows a phantom (dirty-seeded, recencyRank null) way's dirty bit but hides its tag/data as dashes", async () => {
    // The program hasn't actually touched this way yet — only the D bit
    // is real; the tag/words must never leak (there's nothing meaningful
    // to reveal, and it would look like real data otherwise).
    seedOneLevel();
    mockCacheLevel(
      { associativity: 1, blockSize: 4, numSets: 1, wordsPerLine: 1 },
      [{ setIndex: 0, ways: [{ valid: true, dirty: true, tag: 0xabc, recencyRank: null, words: [] }] }],
    );
    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(screen.queryByText("0x00000ABC")).toBeNull();
    const dashes = await screen.findAllByText("-");
    expect(dashes.length).toBeGreaterThan(0);
    const rowCells = container.querySelectorAll(".simulation-cache-set-block .simulation-cache-cell");
    expect(rowCells[1]?.textContent).toBe("1");
  });
});

describe("CacheLevelView — set-associative layout", () => {
  it("stacks a set's ways vertically, showing the set index only once", async () => {
    seedOneLevel();
    mockCacheLevel(
      { associativity: 2, blockSize: 4, numSets: 1, wordsPerLine: 1 },
      [
        {
          setIndex: 0,
          ways: [
            { valid: true, dirty: false, tag: 1, recencyRank: 0, words: [0x11111111] },
            { valid: true, dirty: true, tag: 2, recencyRank: 1, words: [0x22222222] },
          ],
        },
      ],
    );

    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(await screen.findByText("0x11111111")).toBeTruthy();
    expect(screen.getByText("0x22222222")).toBeTruthy();

    // The set index appears exactly once for the whole 2-way block, not
    // once per way, and it spans both way-rows.
    const setCells = container.querySelectorAll(".simulation-cache-set-index");
    expect(setCells.length).toBe(1);
    expect(setCells[0].textContent).toBe("0");
    expect((setCells[0] as HTMLElement).style.gridRow).toBe("1 / span 2");
  });
});

describe("CacheLevelView — windowed scrolling over large levels", () => {
  it("pages through a level far larger than could ever be fetched in one call", async () => {
    seedOneLevel();
    const info: CacheLevelInfo = { associativity: 1, blockSize: 4, numSets: 5_000_000, wordsPerLine: 1 };
    invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
      if (cmd === "get_cache_level_info") return Promise.resolve(info);
      if (cmd === "get_cache_sets") {
        const startSet = args.startSet as number;
        const count = args.count as number;
        const sets: CacheSet[] = Array.from({ length: count }, (_, i) => ({
          setIndex: startSet + i,
          ways: [{ valid: true, dirty: false, tag: startSet + i, recencyRank: 0, words: [startSet + i] }],
        }));
        return Promise.resolve(sets);
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    // No call ever requests anywhere near the full 5,000,000 sets.
    for (const call of invokeMock.mock.calls) {
      if (call[0] === "get_cache_sets") {
        expect((call[1] as { count: number }).count).toBeLessThan(1000);
      }
    }

    invokeMock.mockClear();
    const scrollEl = container.querySelector(".simulation-cache-body") as HTMLDivElement;
    // Scroll to near the bottom of the sliding window to trigger a re-anchor.
    scrollEl.scrollTop = 520_000 - 100;
    await fireEvent.scroll(scrollEl);
    await flush();

    const [, args] = invokeMock.mock.calls.find((call) => call[0] === "get_cache_sets") as [string, { startSet: number }];
    // Re-anchored well past the initial window — proves scrolling isn't
    // capped at whatever the first window happened to cover.
    expect(args.startSet).toBeGreaterThan(1000);
  });
});
