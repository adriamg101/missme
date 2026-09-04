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
  isPhantom: boolean;
  orderValue: number | null;
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

// get_cache_level_info only returns metadata, while get_cache_sets fetches
// pages of the actual set data. This fakes both, and answers get_cache_sets
// by slicing out the requested [startSet, startSet+count) range from the
// given full `allSets` array, the same way the real backend would.
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

// The Set index cell occupies rowCells[0] (it's written once per set block,
// before the way loop), so a way's own cells start after it. header.length
// is Set + one way's worth of columns (V/D/LRU/Tag/words), which is exactly
// the offset of way 1's first cell within a two-way block.
function way1FirstCellIndex(container: Element): number {
  const header = container.querySelectorAll(".simulation-cache-grid-header .simulation-cache-cell");
  return header.length;
}

function defaultInfo(): CacheLevelInfo {
  return { associativity: 1, blockSize: 4, numSets: 2, wordsPerLine: 1 };
}

function defaultSets(): CacheSet[] {
  return [
    { setIndex: 0, ways: [{ valid: true, dirty: false, tag: 5, isPhantom: false, orderValue: 0, words: [0xdeadbeef] }] },
    { setIndex: 1, ways: [{ valid: false, dirty: false, tag: 0, isPhantom: false, orderValue: null, words: [] }] },
  ];
}

// When the component loads, it fetches the level info, resets the window,
// waits for the page to update so it can measure the cache body element,
// fetches the visible sets, and starts listening for events. That's a lot of
// small steps happening one after another, so waiting a fixed small number
// of times isn't reliable. This just waits generously to be safe.
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
    // The tag is padded with zeros to 8 hex digits, same as the words.
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

    // Set 1 in defaultSets() is invalid, so its cells should show "-", not
    // be blank, and should have the empty-cell class.
    const dashes = container.querySelectorAll(".simulation-cache-cell-empty");
    expect(dashes.length).toBeGreaterThan(0);
    expect(Array.from(dashes).every((cell) => cell.textContent === "-")).toBe(true);
  });

  it("shows a phantom (dirty-seeded, never-accessed) way's dirty bit but hides its tag/data as dashes", async () => {
    // The program hasn't actually touched this way yet, only the dirty bit
    // is real. The tag and words must never be shown, since there's nothing
    // meaningful there and it would look like real data otherwise.
    seedOneLevel();
    mockCacheLevel(
      { associativity: 1, blockSize: 4, numSets: 1, wordsPerLine: 1 },
      [{ setIndex: 0, ways: [{ valid: true, dirty: true, tag: 0xabc, isPhantom: true, orderValue: null, words: [] }] }],
    );
    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(screen.queryByText("0x00000ABC")).toBeNull();
    const rowCells = container.querySelectorAll(".simulation-cache-set-block .simulation-cache-cell");
    // rowCells[0] is the Set index cell; the phantom way's own cells follow:
    // V, then D, then dashes for LRU/Tag/Offset/words - every remaining
    // column in the block, none left blank.
    expect(rowCells[1]?.textContent).toBe("1"); // V
    expect(rowCells[2]?.textContent).toBe("1"); // D
    const dashes = container.querySelectorAll(".simulation-cache-set-block .simulation-cache-cell-empty");
    expect(dashes.length).toBe(11); // LRU, Tag, Offset, and 8 word columns
    expect(Array.from(dashes).every((cell) => cell.textContent === "-")).toBe(true);
  });
});

describe("CacheLevelView — V column", () => {
  it("shows an explicit V column with 1 for a real way and 0 for an invalid one", async () => {
    seedOneLevel();
    mockCacheLevel(
      { associativity: 2, blockSize: 4, numSets: 1, wordsPerLine: 1 },
      [
        {
          setIndex: 0,
          ways: [
            { valid: true, dirty: false, tag: 1, isPhantom: false, orderValue: 0, words: [0x11111111] },
            { valid: false, dirty: false, tag: 0, isPhantom: false, orderValue: null, words: [] },
          ],
        },
      ],
    );
    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(await screen.findByText("V")).toBeTruthy(); // the new header cell
    const rowCells = container.querySelectorAll(".simulation-cache-set-block .simulation-cache-cell");
    // columns are: Set(spans, index 0), then per-way V, D, LRU, Tag, words...
    expect(rowCells[1]?.textContent).toBe("1"); // way 0's V
    expect(rowCells[way1FirstCellIndex(container)]?.textContent).toBe("0"); // way 1's V
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
            { valid: true, dirty: false, tag: 1, isPhantom: false, orderValue: 0, words: [0x11111111] },
            { valid: true, dirty: true, tag: 2, isPhantom: false, orderValue: 1, words: [0x22222222] },
          ],
        },
      ],
    );

    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(await screen.findByText("0x11111111")).toBeTruthy();
    expect(screen.getByText("0x22222222")).toBeTruthy();

    // The set index should appear only once for the whole 2-way block, not
    // once per way, and it should span both way rows.
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
          ways: [{ valid: true, dirty: false, tag: startSet + i, isPhantom: false, orderValue: 0, words: [startSet + i] }],
        }));
        return Promise.resolve(sets);
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    // No request should ever ask for anywhere near the full 5,000,000 sets.
    for (const call of invokeMock.mock.calls) {
      if (call[0] === "get_cache_sets") {
        expect((call[1] as { count: number }).count).toBeLessThan(1000);
      }
    }

    invokeMock.mockClear();
    const scrollEl = container.querySelector(".simulation-cache-body") as HTMLDivElement;
    // Scroll to near the bottom of the visible window to force it to reload around a new position.
    scrollEl.scrollTop = 520_000 - 100;
    await fireEvent.scroll(scrollEl);
    await flush();

    const [, args] = invokeMock.mock.calls.find((call) => call[0] === "get_cache_sets") as [string, { startSet: number }];
    // The new position is well past the initial window, which shows that
    // scrolling isn't limited to whatever the first window happened to cover.
    expect(args.startSet).toBeGreaterThan(1000);
  });
});

describe("CacheLevelView — wide lines split into 8-word sub-rows", () => {
  it("splits a line bigger than 8 words into multiple sub-rows with an Offset column", async () => {
    seedOneLevel();
    mockCacheLevel(
      { associativity: 1, blockSize: 64, numSets: 1, wordsPerLine: 16 },
      [{ setIndex: 0, ways: [{ valid: true, dirty: false, tag: 1, isPhantom: false, orderValue: 0, words: Array.from({ length: 16 }, (_, i) => i) }] }],
    );
    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(await screen.findByText("Offset")).toBeTruthy();
    expect(await screen.findByText("0x0000")).toBeTruthy(); // first sub-row's offset
    expect(await screen.findByText("0x0020")).toBeTruthy(); // second sub-row's offset (8 words * 4 bytes)
    // Only 8 word-header columns ever show, never one per real word.
    const headerWordCells = container.querySelectorAll(".simulation-cache-grid-header .simulation-cache-cell-word");
    expect(headerWordCells.length).toBe(8);
  });

  it("shows a dash instead of a made-up word for a column past the line's real word count", async () => {
    seedOneLevel();
    mockCacheLevel(
      { associativity: 1, blockSize: 4, numSets: 1, wordsPerLine: 1 },
      [{ setIndex: 0, ways: [{ valid: true, dirty: false, tag: 1, isPhantom: false, orderValue: 0, words: [0xdeadbeef] }] }],
    );
    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(await screen.findByText("0xDEADBEEF")).toBeTruthy();
    // 7 of the 8 word columns in this single sub-row have no real data.
    const dashes = container.querySelectorAll(".simulation-cache-set-block .simulation-cache-cell-empty");
    expect(dashes.length).toBe(7);
  });
});

describe("CacheLevelView — replacement policy column", () => {
  function seedLevelWithPolicy(replacement: string) {
    const store = useHierarchyConfigStore();
    store.hierarchy = [
      {
        id: "l1",
        harvard: false,
        unified: { exp: 1, config: { blockSize: 4, associativity: 1, replacement, writePolicy: "WriteBack", writeAllocate: true, hitTime: 2, dirtyPercent: 0 } },
        inclusion: "NonInclusive",
        busWidth: 32,
      },
    ] as unknown as ReturnType<typeof useHierarchyConfigStore>["hierarchy"];
  }

  it("labels the order column after the active policy (FIFO)", async () => {
    seedLevelWithPolicy("FIFO");
    mockCacheLevel(defaultInfo(), defaultSets());
    render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(await screen.findByText("FIFO")).toBeTruthy();
    expect(screen.queryByText("LRU")).toBeNull();
  });

  it("hides the order column entirely when the policy is Random", async () => {
    seedLevelWithPolicy("Random");
    mockCacheLevel(defaultInfo(), [
      { setIndex: 0, ways: [{ valid: true, dirty: false, tag: 5, isPhantom: false, orderValue: null, words: [0xdeadbeef] }] },
      { setIndex: 1, ways: [{ valid: false, dirty: false, tag: 0, isPhantom: false, orderValue: null, words: [] }] },
    ]);
    const { container } = render(CacheLevelView, { props: { levelId: "l1", side: "unified" } });
    await flush();

    expect(screen.queryByText("LRU")).toBeNull();
    expect(screen.queryByText("FIFO")).toBeNull();
    expect(screen.queryByText("LFU")).toBeNull();
    // Real data must still show normally, just with no order column.
    expect(await screen.findByText("0xDEADBEEF")).toBeTruthy();
    const headerCells = container.querySelectorAll(".simulation-cache-grid-header .simulation-cache-cell");
    expect(headerCells.length).toBe(13); // Set, V, D, Tag, Offset, 8 words - one fewer than the default 14
  });
});
