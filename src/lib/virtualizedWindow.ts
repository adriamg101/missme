import { computed, onBeforeUnmount, ref, type ComputedRef, type Ref } from "vue";
import { computeVirtualRange, type VirtualRange } from "./virtualScroll";

export interface VirtualizedWindowOptions {
  // A plain number for views where every row is the same fixed height
  // (RAM, Frames, Disk). A function for views where the row height depends
  // on data that only arrives after mount (the TLB's associativity isn't
  // known until `get_tlb_info` resolves, and each set's block is
  // `associativity * ROW_HEIGHT` tall, mirroring CacheLevelView.vue's
  // existing `setBlockHeight`).
  rowHeight: number | (() => number);
  windowRows: number;
  recenterMarginRows: number;
  totalRows: () => number;
  fetchPage: (startIndex: number, count: number) => void;
}

export interface VirtualizedWindow {
  containerRef: Ref<HTMLDivElement | null>;
  scrollTop: Ref<number>;
  baseIndex: Ref<number>;
  range: ComputedRef<VirtualRange>;
  visibleIndices: ComputedRef<number[]>;
  onScroll: (event: Event) => void;
  jumpTo: (targetIndex: number) => void;
  resetWindow: () => void;
  measureViewport: () => void;
}

// Shared by every windowed grid view (RAM, cache, TLB, frames, page table,
// disk): shows only a bounded window of rows at a time and quietly slides
// that window when the user scrolls near either edge, instead of trying to
// represent a possibly enormous address or set range as one giant scroll
// area. See RamView.vue's original comment for why this is necessary.
export function useVirtualizedWindow(options: VirtualizedWindowOptions): VirtualizedWindow {
  const { rowHeight, windowRows, recenterMarginRows, totalRows, fetchPage } = options;

  function currentRowHeight(): number {
    return typeof rowHeight === "function" ? rowHeight() : rowHeight;
  }

  const containerRef = ref<HTMLDivElement | null>(null);
  const scrollTop = ref(0);
  const viewportHeight = ref(0);
  const baseIndex = ref(0);

  // The window is only ever as tall as it needs to be: when the real total
  // (e.g. how many frames actually exist) is smaller than the configured
  // sliding-window size, the scrollable area must match the real total, or
  // the user could scroll far past the last real row into empty space.
  const range = computed(() =>
    computeVirtualRange(scrollTop.value, viewportHeight.value, currentRowHeight(), Math.min(windowRows, totalRows())),
  );
  const visibleIndices = computed(() =>
    Array.from({ length: Math.max(0, range.value.end - range.value.start) }, (_, i) => baseIndex.value + range.value.start + i),
  );

  let ignoreNextScrollEvent = false;

  function setContainerScrollTop(value: number) {
    scrollTop.value = value;
    if (containerRef.value) {
      ignoreNextScrollEvent = true;
      containerRef.value.scrollTop = value;
    }
  }

  function fetchVisibleWindow() {
    const start = range.value.start;
    const count = Math.max(0, range.value.end - start);
    if (count === 0) return;
    fetchPage(baseIndex.value + start, count);
  }

  function recenterOn(topAbsoluteIndex: number) {
    const newBase = Math.max(0, topAbsoluteIndex - Math.floor(windowRows / 2));
    const delta = newBase - baseIndex.value;
    if (delta === 0) return;
    baseIndex.value = newBase;
    setContainerScrollTop(scrollTop.value - delta * currentRowHeight());
  }

  function maybeRecenter() {
    const nearTop = range.value.start < recenterMarginRows && baseIndex.value > 0;
    const nearBottom = range.value.end > Math.min(windowRows, totalRows()) - recenterMarginRows;
    if (!nearTop && !nearBottom) return;
    const topAbsoluteIndex = baseIndex.value + Math.floor(scrollTop.value / currentRowHeight());
    recenterOn(topAbsoluteIndex);
  }

  let scrollEndTimeout: ReturnType<typeof setTimeout> | null = null;

  function snapScrollToWholePixel() {
    if (!containerRef.value) return;
    const current = containerRef.value.scrollTop;
    const floored = Math.floor(current);
    if (current !== floored) setContainerScrollTop(floored);
  }

  function onScroll(event: Event) {
    if (ignoreNextScrollEvent) {
      ignoreNextScrollEvent = false;
      return;
    }
    scrollTop.value = (event.target as HTMLDivElement).scrollTop;
    maybeRecenter();
    fetchVisibleWindow();
    if (scrollEndTimeout) clearTimeout(scrollEndTimeout);
    scrollEndTimeout = setTimeout(snapScrollToWholePixel, 100);
  }

  function jumpTo(targetIndex: number) {
    const maxIndex = Math.max(0, totalRows() - 1);
    const clamped = Math.max(0, Math.min(targetIndex, maxIndex));
    const newBase = Math.max(0, clamped - Math.floor(windowRows / 2));
    baseIndex.value = newBase;
    setContainerScrollTop((clamped - newBase) * currentRowHeight());
    fetchVisibleWindow();
  }

  function resetWindow() {
    baseIndex.value = 0;
    scrollTop.value = 0;
    if (containerRef.value) containerRef.value.scrollTop = 0;
  }

  function measureViewport() {
    if (containerRef.value) viewportHeight.value = containerRef.value.clientHeight;
  }

  onBeforeUnmount(() => {
    if (scrollEndTimeout) clearTimeout(scrollEndTimeout);
  });

  return { containerRef, scrollTop, baseIndex, range, visibleIndices, onScroll, jumpTo, resetWindow, measureViewport };
}
