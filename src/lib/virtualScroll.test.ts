import { describe, it, expect } from "vitest";
import { computeVirtualRange } from "./virtualScroll";

describe("computeVirtualRange", () => {
  it("returns an empty range for zero total rows", () => {
    expect(computeVirtualRange(0, 500, 20, 0)).toEqual({ start: 0, end: 0, totalHeight: 0 });
  });

  it("computes the visible range at the top of the list", () => {
    const range = computeVirtualRange(0, 100, 20, 1000, 2);
    // 5 rows fit on screen (100/20), plus 2 extra rows rendered below, and it can't go below 0 at the top
    expect(range.start).toBe(0);
    expect(range.end).toBe(7);
    expect(range.totalHeight).toBe(20000);
  });

  it("computes the visible range scrolled into the middle of the list", () => {
    const range = computeVirtualRange(500, 100, 20, 1000, 2);
    // first visible row is 500/20 = 25, minus 2 extra rows kept above = 23
    expect(range.start).toBe(23);
    // 5 visible rows + 2 extra rows below = 32
    expect(range.end).toBe(32);
  });

  it("clamps the end of the range to totalRows", () => {
    const range = computeVirtualRange(19900, 100, 20, 1000, 2);
    expect(range.start).toBe(993);
    expect(range.end).toBe(1000);
  });

  it("defaults overscan to 5 when not provided", () => {
    const range = computeVirtualRange(0, 100, 20, 1000);
    expect(range.end).toBe(10); // 5 visible rows + 5 extra rows by default
  });
});
