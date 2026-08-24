export interface VirtualRange {
  start: number;
  end: number;
  totalHeight: number;
}

export function computeVirtualRange(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  totalRows: number,
  overscan = 5,
): VirtualRange {
  if (totalRows <= 0 || rowHeight <= 0) {
    return { start: 0, end: 0, totalHeight: 0 };
  }

  const firstVisible = Math.floor(scrollTop / rowHeight);
  const visibleCount = Math.ceil(viewportHeight / rowHeight);
  const start = Math.max(0, firstVisible - overscan);
  const end = Math.min(totalRows, firstVisible + visibleCount + overscan);

  return { start, end, totalHeight: totalRows * rowHeight };
}
