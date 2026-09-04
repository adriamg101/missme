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

// This must match the block size the backend already uses when it steps through memory.
const RAM_BLOCK_SIZE = 32;
const ROW_HEIGHT = 26;
const MAX_CACHED_BLOCKS = 4000;

// Shows only a smaller window of rows and quietly slides it as the user scrolls, so it never has to draw the whole address space at once.
const WINDOW_ROWS = 20_000;
// Repositions the window before the user reaches its edge, so it is never seen happening.
const RECENTER_MARGIN_ROWS = 500;
const totalBlocks = WINDOW_ROWS;

function absoluteRow(address: number): number {
  return Math.floor(address / RAM_BLOCK_SIZE);
}

// Places the target row in the middle of the window, so there is room to scroll either way.
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

// Blocks are stored by their real address, so they stay valid even after the window moves.
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

// Stops moving the scroll position ourselves from triggering our own scroll handling again.
let ignoreNextScrollEvent = false;

function setContainerScrollTop(value: number) {
  scrollTop.value = value;
  if (containerRef.value) {
    ignoreNextScrollEvent = true;
    containerRef.value.scrollTop = value;
  }
}

// Moves the window so the row at the top of the screen ends up roughly in the middle.
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

// Snaps the scroll position back to a whole pixel once scrolling stops, to stop flickering gaps between cells.
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

// Deliberately moves the view so the target address ends up in the middle of the window.
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

// A word is always padded to 8 hex digits, a block address is left unpadded.
function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

// A word is always 4 bytes, so a word column's byte offset is always its index times 4.
const WORD_SIZE = 4;
const wordsPerGroup = computed(() => (store.isa === "RV32I" ? 1 : 2));

function offsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * WORD_SIZE).toString(16).toUpperCase().padStart(2, "0")}`;
}

// Marks where a new group of words starts, for a border only.
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
    // The element always starts scrolled to the top, so we move it to the centered starting position.
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

/* This handles horizontal scrolling only. Vertical scrolling is handled
   separately by .simulation-ram-body below, on purpose. We used to have
   one scroll area with a header that stayed stuck to the top while
   scrolling, but with a grid this tall (many thousands of rows), that
   caused a visible flicker as the browser kept recalculating the header's
   position. Keeping the header out of the scrolling area entirely avoids
   that problem completely. */
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
