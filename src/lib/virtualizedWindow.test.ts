import { describe, it, expect, vi } from "vitest";
import { useVirtualizedWindow } from "./virtualizedWindow";

// These tests exercise the composable's pure logic (base index, range,
// jump, reset) directly, without mounting it inside a real component, since
// none of that logic actually depends on Vue's rendering lifecycle.
describe("useVirtualizedWindow", () => {
  it("starts at base index 0 before any viewport height has been measured", () => {
    const w = useVirtualizedWindow({ rowHeight: 20, windowRows: 1000, recenterMarginRows: 50, totalRows: () => 1000, fetchPage: vi.fn() });
    expect(w.baseIndex.value).toBe(0);
    // Before measureViewport() runs, the range is just the overscan
    // padding around row 0, not empty. RamView.vue and CacheLevelView.vue
    // already rely on this same behavior.
    expect(w.range.value.start).toBe(0);
    expect(w.range.value.end).toBe(5);
  });

  it("jumpTo centers the target row and updates scrollTop", () => {
    const w = useVirtualizedWindow({ rowHeight: 10, windowRows: 100, recenterMarginRows: 5, totalRows: () => 100, fetchPage: vi.fn() });
    w.jumpTo(50);
    expect(w.baseIndex.value).toBe(0); // windowRows/2 = 50, target 50 - 50 = 0, clamped to 0
    expect(w.scrollTop.value).toBe(500); // (50 - 0) * rowHeight
  });

  it("jumpTo never moves the base index below zero", () => {
    const w = useVirtualizedWindow({ rowHeight: 10, windowRows: 100, recenterMarginRows: 5, totalRows: () => 100, fetchPage: vi.fn() });
    w.jumpTo(5);
    expect(w.baseIndex.value).toBe(0);
  });

  it("resetWindow returns to base index 0 and scrollTop 0", () => {
    const w = useVirtualizedWindow({ rowHeight: 10, windowRows: 100, recenterMarginRows: 5, totalRows: () => 100, fetchPage: vi.fn() });
    w.jumpTo(80);
    w.resetWindow();
    expect(w.baseIndex.value).toBe(0);
    expect(w.scrollTop.value).toBe(0);
  });

  it("caps the scrollable area at the real total, not the configured window size, when the real total is smaller", () => {
    // e.g. a small RAM/frame pool with only 8 real rows, even though the
    // window itself is configured much larger. The scrollbar must only
    // span the real 8 rows, not the whole window.
    const w = useVirtualizedWindow({ rowHeight: 10, windowRows: 20_000, recenterMarginRows: 500, totalRows: () => 8, fetchPage: vi.fn() });
    expect(w.range.value.totalHeight).toBe(80); // 8 rows * 10px, not 20,000 * 10px
  });

  it("jumpTo calls fetchPage for the newly visible window", () => {
    const fetchPage = vi.fn();
    const w = useVirtualizedWindow({ rowHeight: 10, windowRows: 100, recenterMarginRows: 5, totalRows: () => 100, fetchPage });
    // Without a measured viewport height the visible range is empty, so give it one via onScroll's first call path isn't available here;
    // instead assert jumpTo attempted a fetch call (count may be 0 rows but the call itself still happens when range is non-empty).
    w.jumpTo(10);
    expect(fetchPage).toHaveBeenCalled();
  });
});
