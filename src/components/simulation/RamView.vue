<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useCompileConfigStore } from "../../stores/compileConfig";
import { computeVirtualRange } from "../../lib/virtualScroll";
import { touchAndPrune } from "../../lib/lruCache";

const store = useHierarchyConfigStore();
const compileConfigStore = useCompileConfigStore();

// Must match DEFAULT_BLOCK_SIZE in src-tauri/src/controllers/simulation_controller.rs,
// which independently assumes this same stride when stepping between blocks.
const RAM_BLOCK_SIZE = 32;
const ROW_HEIGHT = 26;
const MAX_CACHED_BLOCKS = 4000;

// One grid, one scrollbar — but it can't literally span the whole address
// space as a single proportional scrollbar: the gap between .text and
// .data alone is ~252MB, which at any usable row height blows past the
// browser's max element height (~33.5M px in Chromium). So instead the
// scrollbar represents a sliding WINDOW_ROWS-row window anchored at
// `baseAddress`; scrolling near either edge of that window silently
// re-anchors it (adjusting scrollTop by the same amount so nothing visibly
// jumps), so the user can keep scrolling indefinitely in either direction
// without ever hitting a dead end — this is what makes it feel unified
// rather than capped at whatever `ram.size` happens to be.
// Kept much smaller than "safely under the browser's max element height"
// would allow (that limit is ~33.5M px) — at multi-million-px scrollTop
// values, browsers lose floating-point precision recomputing `position:
// sticky` on every scroll frame, causing a visible 1px jitter ("vibration")
// while actively scrolling that disappears at rest. Staying in the low
// hundreds-of-thousands of px avoids that entirely, at the cost of
// re-anchoring more often during a long scroll (cheap, invisible either way).
const WINDOW_ROWS = 20_000; // 20_000 * ROW_HEIGHT = 520,000px
const RECENTER_MARGIN_ROWS = 500; // trigger a re-anchor well before the literal edge, so it's never visible
const totalBlocks = WINDOW_ROWS;

function absoluteRow(address: number): number {
  return Math.floor(address / RAM_BLOCK_SIZE);
}

// Centers `targetRow` in the window, clamped so the window never anchors
// below address 0. Shared by the initial mount position and every jump, so
// both leave equal room to scroll either direction — a naive `baseAddress
// = targetAddress` (row 0 of the window) would leave NO room above,
// making maybeRecenter's nearTop condition true almost immediately on the
// very next scroll tick.
function centeredBaseRow(targetRow: number): number {
  return Math.max(0, targetRow - Math.floor(WINDOW_ROWS / 2));
}

const initialTargetRow = absoluteRow(compileConfigStore.textBase);
const initialBaseRow = centeredBaseRow(initialTargetRow);

const baseAddress = ref(initialBaseRow * RAM_BLOCK_SIZE);

interface RamBlock {
  address: number;
  words: number[];
  populated: boolean;
  dirty: boolean;
}

// Keyed by absolute row number (address / RAM_BLOCK_SIZE), NOT by
// window-relative row index — so cached blocks stay valid across a
// re-anchor (only which rows are currently in view changes, not what a
// given address's cached content is). touchAndPrune's LRU eviction is what
// actually keeps memory bounded as the window slides: rows far outside the
// current view stop being touched and eventually get pruned.
const blocks = reactive(new Map<number, RamBlock>());
const containerRef = ref<HTMLDivElement | null>(null);
const scrollTop = ref((initialTargetRow - initialBaseRow) * ROW_HEIGHT);
const viewportHeight = ref(0);

const baseRow = computed(() => absoluteRow(baseAddress.value));

const range = computed(() => computeVirtualRange(scrollTop.value, viewportHeight.value, ROW_HEIGHT, totalBlocks));

async function fetchVisibleWindow() {
  const start = range.value.start;
  const count = Math.max(0, range.value.end - start);
  if (count === 0) return;
  const startAddress = baseAddress.value + start * RAM_BLOCK_SIZE;
  try {
    const result = await invoke<RamBlock[]>("get_ram_blocks", { startAddress, count });
    const touched: number[] = [];
    result.forEach((block) => {
      const key = absoluteRow(block.address);
      blocks.set(key, block);
      touched.push(key);
    });
    touchAndPrune(blocks, touched, MAX_CACHED_BLOCKS);
  } catch (error) {
    console.error("Failed to fetch RAM blocks:", error);
  }
}

// Writing to a DOM element's scrollTop dispatches its own native 'scroll'
// event asynchronously — without this guard, that synthetic event would
// feed back into onScroll, which could re-run maybeRecenter and jitter
// the window back and forth on every real scroll tick near an edge (the
// "vibration"). The one scroll event caused by our own assignment is
// swallowed here; scrollTop.value/baseAddress are already correct because
// the caller set them itself before touching the DOM.
let ignoreNextScrollEvent = false;

function setContainerScrollTop(value: number) {
  scrollTop.value = value;
  if (containerRef.value) {
    ignoreNextScrollEvent = true;
    containerRef.value.scrollTop = value;
  }
}

// Re-anchors the window so the row currently at the top of the viewport
// ends up roughly in the middle, giving equal room to keep scrolling in
// either direction. Never anchors below address 0.
function recenterOn(topAbsoluteRow: number) {
  const newBaseRow = centeredBaseRow(topAbsoluteRow);
  const deltaRows = newBaseRow - baseRow.value;
  if (deltaRows === 0) return;
  baseAddress.value = newBaseRow * RAM_BLOCK_SIZE;
  setContainerScrollTop(scrollTop.value - deltaRows * ROW_HEIGHT);
}

function maybeRecenter() {
  const nearTop = range.value.start < RECENTER_MARGIN_ROWS && baseRow.value > 0;
  const nearBottom = range.value.end > totalBlocks - RECENTER_MARGIN_ROWS;
  if (!nearTop && !nearBottom) return;
  const topAbsoluteRow = baseRow.value + Math.floor(scrollTop.value / ROW_HEIGHT);
  recenterOn(topAbsoluteRow);
}

// A fractional scrollTop (e.g. from trackpad/smooth-scroll momentum, or a
// non-integer devicePixelRatio) puts the viewport at a non-whole-pixel
// offset while every row's own `top` is an integer — the browser then has
// to round each bordered cell's edges independently against that
// fractional origin, and can round adjacent cells differently, showing up
// as flickering hairline gaps between them while scrolling settles. Once
// scrolling stops, snap straight to a whole pixel (always floor — which
// direction doesn't matter, only that it's deterministic) so there's
// nothing left to round inconsistently.
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

// Unlike recenterOn (which preserves whatever's currently visible while
// sliding the window), a jump deliberately moves the viewport: the target
// address ends up at the vertical middle of the window, with equal room to
// scroll either direction from there.
function jumpToAddress(address: number) {
  const targetRow = absoluteRow(Math.max(0, address));
  const newBaseRow = centeredBaseRow(targetRow);
  baseAddress.value = newBaseRow * RAM_BLOCK_SIZE;
  setContainerScrollTop((targetRow - newBaseRow) * ROW_HEIGHT);
  fetchVisibleWindow();
}

function jumpToText() {
  jumpToAddress(compileConfigStore.textBase);
}

function jumpToData() {
  jumpToAddress(compileConfigStore.dataBase);
}

const addressInput = ref("");
const addressInputInvalid = ref(false);

function jumpToInputAddress() {
  const trimmed = addressInput.value.trim();
  const match = /^0x[0-9a-fA-F]+$/.exec(trimmed);
  if (!match) {
    addressInputInvalid.value = true;
    return;
  }
  addressInputInvalid.value = false;
  jumpToAddress(parseInt(trimmed, 16));
}

function hex(value: number): string {
  return (value >>> 0).toString(16);
}

// Words are always the full 4-byte range even when the stored value is
// small (e.g. only the low byte was ever written) — zero-pad to 8 hex
// digits so that stays visually obvious, unlike the block address, which
// doesn't have a fixed width and is left unpadded.
function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

// Word size is fixed at 4 bytes across this engine (see the S4 plan's
// rulings), so a word-column's byte offset is always wordIndex * 4, and the
// architecture's native word size (4 on RV32I, 8 on RV64I) is expressed as a
// number of word-columns to group together.
const WORD_SIZE = 4;
const wordsPerGroup = computed(() => (store.isa === "RV32I" ? 1 : 2));

function offsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * WORD_SIZE).toString(16).toUpperCase().padStart(2, "0")}`;
}

// Only meaningful when a group spans more than one word-column (RV64I) — a
// dashed left border marking where the next native-word group starts.
function isGroupStart(wordIndex: number): boolean {
  return wordsPerGroup.value > 1 && wordIndex > 0 && wordIndex % wordsPerGroup.value === 0;
}

const wordsPerBlock = computed(() => RAM_BLOCK_SIZE / 4);
const rowGridStyle = computed(() => ({
  display: "grid" as const,
  height: `${ROW_HEIGHT}px`,
  gridTemplateColumns: `8rem repeat(${wordsPerBlock.value}, minmax(calc(10ch + 0.6rem), auto))`,
}));

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  if (containerRef.value) {
    viewportHeight.value = containerRef.value.clientHeight;
    // The DOM element itself always starts at scrollTop 0 — sync it to the
    // pre-centered initial position computed above.
    containerRef.value.scrollTop = scrollTop.value;
  }
  await fetchVisibleWindow();

  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "ram_access") return;
    fetchVisibleWindow();
  });
  if (disposed) stopListening();
  else unlisten = stopListening;
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
  if (scrollEndTimeout) clearTimeout(scrollEndTimeout);
});
</script>

<template>
  <div class="simulation-ram-view">
    <div class="simulation-ram-jump-bar">
      <button type="button" title="Jump to .text" @click="jumpToText">.text</button>
      <button type="button" title="Jump to .data" @click="jumpToData">.data</button>
      <input
        type="text"
        class="simulation-ram-jump-input"
        :class="{ 'simulation-ram-jump-input-invalid': addressInputInvalid }"
        placeholder="0x..."
        aria-label="Jump to address"
        v-model="addressInput"
        @keydown.enter="jumpToInputAddress"
        @input="addressInputInvalid = false"
      />
      <button type="button" title="Jump to this address" @click="jumpToInputAddress">Go</button>
    </div>
    <div class="simulation-ram-outer">
      <div class="simulation-ram-grid-wrapper">
        <div class="simulation-ram-grid-header" :style="rowGridStyle">
          <span class="simulation-ram-cell simulation-ram-cell-header">Address</span>
          <span
            v-for="wi in wordsPerBlock"
            :key="wi"
            class="simulation-ram-cell simulation-ram-cell-header simulation-ram-cell-word"
            :class="{ 'simulation-ram-group-start': isGroupStart(wi - 1) }"
          >{{ offsetLabel(wi - 1) }}</span>
        </div>
        <div class="simulation-ram-body" ref="containerRef" @scroll="onScroll">
          <div class="simulation-ram-spacer" :style="{ height: `${range.totalHeight}px`, position: 'relative' }">
            <div
              v-for="index in Array.from({ length: Math.max(0, range.end - range.start) }, (_, i) => range.start + i)"
              :key="baseRow + index"
              class="simulation-ram-row"
              :style="{ position: 'absolute', top: `${index * ROW_HEIGHT}px`, ...rowGridStyle }"
            >
              <template v-if="blocks.get(baseRow + index)">
                <span class="simulation-ram-cell simulation-ram-address">0x{{ hex(blocks.get(baseRow + index)!.address) }}</span>
                <span
                  v-for="(word, wi) in blocks.get(baseRow + index)!.words"
                  :key="wi"
                  class="simulation-ram-cell simulation-ram-cell-word"
                  :class="{
                    'simulation-ram-cell-empty': !blocks.get(baseRow + index)!.populated,
                    'simulation-ram-group-start': isGroupStart(wi),
                  }"
                >{{ blocks.get(baseRow + index)!.populated ? hexWord(word) : "-" }}</span>
              </template>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.simulation-ram-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
}

.simulation-ram-jump-bar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  border-bottom: 1px solid var(--secondary);
}

.simulation-ram-jump-bar button {
  padding: 0.3rem 0.7rem;
  border-radius: 6px;
  border: 1px solid var(--secondary);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 0.8rem;
  cursor: pointer;
}

.simulation-ram-jump-bar button:hover {
  border-color: var(--accent);
}

.simulation-ram-jump-input {
  flex: 0 1 9rem;
  min-width: 0;
  padding: 0.3rem 0.5rem;
  border-radius: 6px;
  border: 1px solid var(--secondary);
  background: var(--background);
  color: var(--text);
  font-family: monospace;
  font-size: 0.8rem;
  box-sizing: border-box;
}

.simulation-ram-jump-input-invalid {
  border-color: var(--diagram-miss-color);
}

/* Horizontal scroll only — vertical is handled by .simulation-ram-body
   below, deliberately NOT combined into one scroll container. A single
   scroller with a `position: sticky` header was the previous approach,
   but browsers recompute a sticky element's position every scroll frame,
   and at the tall (many-thousand-row) heights this grid can reach, that
   recomputation is prone to visible 1px rounding jitter. Splitting the
   header out into normal (non-sticky, non-scrolling-vertically) flow
   avoids that class of bug entirely, not just mitigates it. */
.simulation-ram-outer {
  flex: 1 1 auto;
  min-height: 0;
  padding: 0.5rem;
  font-size: 0.75rem;
  font-family: monospace;
  overflow-x: auto;
  overflow-y: hidden;
  box-sizing: border-box;
}

.simulation-ram-grid-wrapper {
  display: flex;
  flex-direction: column;
  width: fit-content;
  height: 100%;
  margin: 0 auto;
}

.simulation-ram-grid-header {
  align-items: stretch;
  flex: 0 0 auto;
  background: var(--background);
  box-shadow: 0 2px 4px -2px rgba(0, 0, 0, 0.4);
  width: fit-content;
}

.simulation-ram-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.simulation-ram-row {
  align-items: stretch;
  width: fit-content;
}

.simulation-ram-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
}

.simulation-ram-cell-header {
  font-weight: 600;
  opacity: 0.75;
}

.simulation-ram-cell-word {
  padding: 0.1rem 0.3rem;
}

.simulation-ram-group-start {
  border-left: 2px dashed var(--secondary);
}

.simulation-ram-address {
  color: var(--text-muted, var(--text));
}

.simulation-ram-cell-empty {
  color: var(--text-muted, var(--text));
  opacity: 0.4;
}
</style>
