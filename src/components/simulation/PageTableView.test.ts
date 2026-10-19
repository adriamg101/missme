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

import PageTableView from "./PageTableView.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

interface PteRow {
  level: number;
  index: number;
  key: number;
  pointsToOrPpn: number;
  present: boolean;
}

// `levelBits` index 0 = leaf. `rows` maps a level number to that whole
// level's real entries (across every branch); the mock filters them by
// parentKey/range itself, the same way the real backend does.
function touchedRows(levelBits: number[], rows: Record<number, PteRow[]>, level: number, parentKey: number | null | undefined) {
  const ownBits = levelBits[level - 1] ?? 0;
  return (rows[level] ?? [])
    .filter((r) => (parentKey === null || parentKey === undefined ? true : r.key >> ownBits === parentKey))
    .slice()
    .sort((a, b) => a.index - b.index);
}

function mockPageTable(levelBits: number[], rows: Record<number, PteRow[]>) {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "get_page_table_levels") return Promise.resolve(levelBits.length);
    if (cmd === "get_page_table_level_bits") return Promise.resolve(levelBits);
    if (cmd === "get_page_table_entry_count") {
      const level = args.level as number;
      const parentKey = args.parentKey as number | null;
      return Promise.resolve(touchedRows(levelBits, rows, level, parentKey).length);
    }
    if (cmd === "get_page_table_entries") {
      const level = args.level as number;
      const parentKey = args.parentKey as number | null;
      const startIndex = args.startIndex as number;
      const count = args.count as number;
      const matches = touchedRows(levelBits, rows, level, parentKey).slice(startIndex, startIndex + count);
      return Promise.resolve(matches);
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

describe("PageTableView", () => {
  it("shows the root table's level number and entries on mount", async () => {
    // 2 levels, 4 bits each: level 2 is the root.
    mockPageTable([4, 4], { 2: [{ level: 2, index: 0, key: 0, pointsToOrPpn: 0, present: true }] });
    render(PageTableView);
    await flush();
    expect(await screen.findByText("Level 2")).toBeTruthy();
  });

  it("shows a real PPN only at the leaf level", async () => {
    mockPageTable([4, 4], { 2: [{ level: 2, index: 0, key: 0, pointsToOrPpn: 0, present: true }] });
    const { container } = render(PageTableView);
    await flush();
    // The root (non-leaf) level's PPN column must be a dash.
    const row = container.querySelector(".simulation-page-table-row")!;
    expect(row.querySelectorAll(".simulation-page-table-cell-empty").length).toBeGreaterThan(0);
  });

  it("has no back button while viewing the root", async () => {
    mockPageTable([4, 4], { 2: [] });
    render(PageTableView);
    await flush();
    expect(screen.queryByTitle("Back to parent table")).toBeNull();
  });

  it("navigating into a present entry shows the child table and a back button", async () => {
    mockPageTable([4, 4], {
      2: [{ level: 2, index: 0, key: 0, pointsToOrPpn: 0, present: true }],
      1: [{ level: 1, index: 3, key: 3, pointsToOrPpn: 7, present: true }],
    });
    render(PageTableView);
    await flush();

    const arrowButton = await screen.findByTitle("Open this table");
    await arrowButton.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    (arrowButton as HTMLButtonElement).click();
    await flush();

    expect(await screen.findByText("Level 1")).toBeTruthy();
    expect(await screen.findByText("0x7")).toBeTruthy(); // the leaf's real PPN
    expect(screen.queryByTitle("Back to parent table")).toBeTruthy();
  });

  it("the leaf level's table has no arrow column", async () => {
    mockPageTable([4, 4], { 2: [{ level: 2, index: 0, key: 0, pointsToOrPpn: 0, present: true }] });
    const { container } = render(PageTableView);
    await flush();
    (await screen.findByTitle("Open this table")).dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(container.querySelectorAll("[title='Open this table']").length).toBe(0);
  });

  it("the back button returns to the parent table", async () => {
    mockPageTable([4, 4], {
      2: [{ level: 2, index: 0, key: 0, pointsToOrPpn: 0, present: true }],
      1: [{ level: 1, index: 3, key: 3, pointsToOrPpn: 7, present: true }],
    });
    render(PageTableView);
    await flush();
    (await screen.findByTitle("Open this table")).dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(await screen.findByText("Level 1")).toBeTruthy();

    (await screen.findByTitle("Back to parent table")).dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(await screen.findByText("Level 2")).toBeTruthy();
    expect(screen.queryByTitle("Back to parent table")).toBeNull();
  });

  it("refetches the current table on a tlb_access event", async () => {
    mockPageTable([4, 4], { 2: [{ level: 2, index: 0, key: 0, pointsToOrPpn: 0, present: true }] });
    render(PageTableView);
    await flush();
    invokeMock.mockClear();

    capturedHandler!({ payload: { kind: "tlb_access", outcome: "miss", sourceLine: 1 } });
    await flush();

    expect(invokeMock).toHaveBeenCalledWith("get_page_table_entries", expect.anything());
  });
});

describe("PageTableView — wide Index and PPN digit width", () => {
  it("still shows the full Index and PPN values, not truncated, when a level or RAM needs more than 8 hex digits", async () => {
    useHierarchyConfigStore().ram.frames = 2 ** 40;
    mockPageTable([40], { 1: [{ level: 1, index: 0, key: 0, pointsToOrPpn: 0, present: true }] });
    render(PageTableView);
    await flush();

    // 40 index bits round up to 10 hex digits; 2^40 frames need 10 PPN digits too.
    expect((await screen.findAllByText("0x0000000000")).length).toBeGreaterThan(0);
  });
});
