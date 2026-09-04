<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { computeVirtualRange } from "../../lib/virtualScroll";
import { touchAndPrune } from "../../lib/lruCache";
import type { ReplacementPolicy } from "../../lib/hierarchy/model";

const props = defineProps<{ levelId: string; side: "unified" | "i" | "d" }>();
const hierarchyStore = useHierarchyConfigStore();

const levelIndex = computed(() => hierarchyStore.hierarchy.findIndex((l) => l.id === props.levelId));

// The active policy for this level and side, read straight from the store.
const activePolicy = computed<ReplacementPolicy | null>(() => {
  const level = hierarchyStore.hierarchy[levelIndex.value];
  if (!level) return null;
  const entry = props.side === "unified" ? level.unified : props.side === "i" ? level.i : level.d;
  return entry?.config.replacement ?? null;
});

// The order column disappears entirely for Random, since there is nothing to show.
const orderColumnLabel = computed(() => {
  if (activePolicy.value === "FIFO") return "FIFO";
  if (activePolicy.value === "LFU") return "LFU";
  return "LRU";
});
const showOrderColumn = computed(() => activePolicy.value !== "Random");

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
interface CacheLevelInfo {
  associativity: number;
  blockSize: number;
  numSets: number;
  wordsPerLine: number;
}

const levelInfo = ref<CacheLevelInfo | null>(null);
const MAX_CACHED_SETS = 4000;

const ROW_HEIGHT = 26;
const WORDS_PER_SUBROW = 8;
const containerRef = ref<HTMLDivElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);

// A line can hold far more words than fit in a readable row, so each way is drawn as several rows of 8 words instead of one wide row.
const subRowsPerLine = computed(() => Math.max(1, Math.ceil((levelInfo.value?.wordsPerLine ?? 1) / WORDS_PER_SUBROW)));

// Every set has the same height, since associativity and block size stay fixed within a level.
const setBlockHeight = computed(() => Math.max(1, levelInfo.value?.associativity ?? 1) * subRowsPerLine.value * ROW_HEIGHT);

// The number of sets can be far too large to scroll all at once, so only a smaller window is shown and quietly slid as the user scrolls.
const MAX_WINDOW_HEIGHT_PX = 520_000;
const windowSets = computed(() => {
  const capped = Math.max(100, Math.floor(MAX_WINDOW_HEIGHT_PX / setBlockHeight.value));
  return Math.min(capped, levelInfo.value?.numSets ?? capped);
});
const RECENTER_MARGIN_FRACTION = 0.025;
const recenterMarginSets = computed(() => Math.max(5, Math.floor(windowSets.value * RECENTER_MARGIN_FRACTION)));

const baseSet = ref(0);

// Sets are stored by their real index so cached sets stay valid even after the window moves.
const sets = reactive(new Map<number, CacheSet>());

const range = computed(() => computeVirtualRange(scrollTop.value, viewportHeight.value, setBlockHeight.value, windowSets.value));
const visibleSetIndices = computed(() =>
  Array.from({ length: Math.max(0, range.value.end - range.value.start) }, (_, i) => baseSet.value + range.value.start + i),
);

async function fetchVisibleWindow() {
  if (levelIndex.value < 0 || !levelInfo.value) return;
  const start = range.value.start;
  const count = Math.max(0, range.value.end - start);
  if (count === 0) return;
  const startSet = baseSet.value + start;
  try {
    const result = await invoke<CacheSet[]>("get_cache_sets", {
      levelIndex: levelIndex.value,
      side: props.side,
      startSet,
      count,
    });
    const touched: number[] = [];
    result.forEach((set) => {
      sets.set(set.setIndex, set);
      touched.push(set.setIndex);
    });
    touchAndPrune(sets, touched, MAX_CACHED_SETS);
  } catch (error) {
    console.error("Failed to fetch cache sets:", error);
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

// Moves the window so the set at the top of the screen ends up roughly in the middle.
function recenterOn(topAbsoluteSet: number) {
  const newBaseSet = Math.max(0, topAbsoluteSet - Math.floor(windowSets.value / 2));
  const deltaSets = newBaseSet - baseSet.value;
  if (deltaSets === 0) return;
  baseSet.value = newBaseSet;
  setContainerScrollTop(scrollTop.value - deltaSets * setBlockHeight.value);
}

function maybeRecenter() {
  const nearTop = range.value.start < recenterMarginSets.value && baseSet.value > 0;
  const nearBottom = range.value.end > windowSets.value - recenterMarginSets.value;
  if (!nearTop && !nearBottom) return;
  const topAbsoluteSet = baseSet.value + Math.floor(scrollTop.value / setBlockHeight.value);
  recenterOn(topAbsoluteSet);
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

// Jumps straight to a set, centering it in the window.
function jumpToSet(targetSet: number) {
  const maxSet = Math.max(0, (levelInfo.value?.numSets ?? 1) - 1);
  const clamped = Math.max(0, Math.min(targetSet, maxSet));
  const newBaseSet = Math.max(0, clamped - Math.floor(windowSets.value / 2));
  baseSet.value = newBaseSet;
  setContainerScrollTop((clamped - newBaseSet) * setBlockHeight.value);
  fetchVisibleWindow();
}

const setInput = ref("");
const setInputInvalid = ref(false);

function jumpToInputSet() {
  const trimmed = setInput.value.trim();
  if (!/^\d+$/.test(trimmed)) {
    setInputInvalid.value = true;
    return;
  }
  setInputInvalid.value = false;
  jumpToSet(Number(trimmed));
}

// A word is always 4 bytes, so a word column's byte offset is always its index times 4.
const WORD_SIZE = 4;
const wordsPerGroup = computed(() => (hierarchyStore.isa === "RV32I" ? 1 : 2));

// Words and the Tag are always padded to 8 hex digits, addresses are left unpadded.
function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

// Which byte within a row of 8 words a given column is.
function offsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * WORD_SIZE).toString(16).toUpperCase().padStart(2, "0")}`;
}

// The starting byte of this row within the whole line.
function subRowOffsetLabel(subRow: number): string {
  return `0x${(subRow * WORDS_PER_SUBROW * WORD_SIZE).toString(16).toUpperCase().padStart(4, "0")}`;
}

function wordAt(way: CacheWay, subRow: number, column: number): number {
  return way.words[subRow * WORDS_PER_SUBROW + column] ?? 0;
}

// A short line does not fill every one of the 8 word columns, so this tells which ones have no real data to show.
function wordExistsAt(subRow: number, column: number): boolean {
  return subRow * WORDS_PER_SUBROW + column < (levelInfo.value?.wordsPerLine ?? 0);
}

// Marks where a new group of words starts, for a border only.
function isGroupStart(wordIndex: number): boolean {
  return wordsPerGroup.value > 1 && wordIndex > 0 && wordIndex % wordsPerGroup.value === 0;
}

// The header and every set block share the same column layout.
const columnsStyle = computed(() => {
  const orderTrack = showOrderColumn.value ? "2.5rem " : "";
  return `4rem 2.5rem 2.5rem ${orderTrack}calc(10ch + 0.6rem) 4rem repeat(${WORDS_PER_SUBROW}, minmax(calc(10ch + 0.6rem), auto))`;
});
const headerGridStyle = computed(() => ({
  display: "grid" as const,
  height: `${ROW_HEIGHT}px`,
  gridTemplateColumns: columnsStyle.value,
}));
const setGridStyle = computed(() => ({
  display: "grid" as const,
  gridTemplateColumns: columnsStyle.value,
  gridAutoRows: `${ROW_HEIGHT}px`,
}));

// Every way takes up the same number of rows, whether it holds real data or not.
function wayRowStart(wayIndex: number): number {
  return 1 + wayIndex * subRowsPerLine.value;
}

// The Tag, Offset, and word columns shift one column to the left when the order column is hidden.
const colTag = computed(() => (showOrderColumn.value ? 5 : 4));
const colOffset = computed(() => colTag.value + 1);
const dashColumns = computed(() => Array.from({ length: 1 + WORDS_PER_SUBROW }, (_, i) => colOffset.value + i));
// The order column's own dash filler cell, only present when it's shown.
const orderDashColumn = computed(() => (showOrderColumn.value ? [4] : []));

// Resets the window back to the top of the level.
function resetWindow() {
  baseSet.value = 0;
  sets.clear();
  scrollTop.value = 0;
  if (containerRef.value) containerRef.value.scrollTop = 0;
}

async function loadLevelInfo() {
  if (levelIndex.value < 0) {
    levelInfo.value = null;
    return;
  }
  try {
    levelInfo.value = await invoke<CacheLevelInfo>("get_cache_level_info", { levelIndex: levelIndex.value, side: props.side });
    resetWindow();
    // Waits one tick so the page can catch up before measuring its height.
    await nextTick();
    if (containerRef.value) viewportHeight.value = containerRef.value.clientHeight;
    await fetchVisibleWindow();
  } catch (error) {
    console.error("Failed to fetch cache level info:", error);
  }
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  await loadLevelInfo();

  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "cache_access") return;
    if (event.payload.levelIndex !== levelIndex.value) return;
    // Only the visible sets could have changed, the level's own settings never change during a run.
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

watch(() => [props.levelId, props.side], loadLevelInfo);
</script>

<template>
  <div class="simulation-cache-level-view">
    <template v-if="levelInfo">
      <div class="simulation-cache-jump-bar">
        <input
          type="text"
          class="simulation-cache-jump-input"
          :class="{ 'simulation-cache-jump-input-invalid': setInputInvalid }"
          placeholder="Set #"
          aria-label="Jump to set"
          v-model="setInput"
          @keydown.enter="jumpToInputSet"
          @input="setInputInvalid = false"
        />
        <button type="button" title="Jump to this set" @click="jumpToInputSet">Go</button>
      </div>
      <div class="simulation-cache-outer">
        <div class="simulation-cache-grid-wrapper">
          <div class="simulation-cache-grid-header" :style="headerGridStyle">
            <span class="simulation-cache-cell simulation-cache-cell-header">Set</span>
            <span class="simulation-cache-cell simulation-cache-cell-header">V</span>
            <span class="simulation-cache-cell simulation-cache-cell-header">D</span>
            <span v-if="showOrderColumn" class="simulation-cache-cell simulation-cache-cell-header">{{ orderColumnLabel }}</span>
            <span class="simulation-cache-cell simulation-cache-cell-header">Tag</span>
            <span class="simulation-cache-cell simulation-cache-cell-header">Offset</span>
            <span
              v-for="wi in WORDS_PER_SUBROW"
              :key="wi"
              class="simulation-cache-cell simulation-cache-cell-header simulation-cache-cell-word"
              :class="{ 'simulation-cache-group-start': isGroupStart(wi - 1) }"
            >{{ offsetLabel(wi - 1) }}</span>
          </div>
          <div class="simulation-cache-body" ref="containerRef" @scroll="onScroll">
            <div class="simulation-cache-spacer" :style="{ height: `${range.totalHeight}px`, position: 'relative' }">
              <div
                v-for="(setIndex, i) in visibleSetIndices"
                :key="setIndex"
                class="simulation-cache-set-block"
                :style="{ position: 'absolute', top: `${(range.start + i) * setBlockHeight}px`, height: `${setBlockHeight}px`, ...setGridStyle }"
              >
                <template v-if="sets.get(setIndex)">
                  <!--
                    The Set number is placed first and stretches down to cover
                    every way's full block (subRowsPerLine rows each) in this
                    set, taking up the first column for the whole block. Every
                    other cell gets an explicit column and row instead of
                    relying on the grid's auto-placement, since each way now
                    spans several rows (one per 8 words) rather than exactly
                    one, which auto-placement can't express on its own.
                  -->
                  <span
                    class="simulation-cache-cell simulation-cache-set-index"
                    :style="{ gridColumn: 1, gridRow: `1 / span ${sets.get(setIndex)!.ways.length * subRowsPerLine}` }"
                  >{{ setIndex }}</span>
                  <template v-for="(way, wi) in sets.get(setIndex)!.ways" :key="wi">
                    <!-- A real line: one that an actual access put there. -->
                    <template v-if="way.valid && !way.isPhantom">
                      <span class="simulation-cache-cell" :style="{ gridColumn: 2, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }">1</span>
                      <span class="simulation-cache-cell" :style="{ gridColumn: 3, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }">{{ way.dirty ? "1" : "0" }}</span>
                      <span v-if="showOrderColumn" class="simulation-cache-cell" :style="{ gridColumn: 4, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }">{{ way.orderValue }}</span>
                      <span class="simulation-cache-cell simulation-cache-cell-word" :style="{ gridColumn: colTag, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }">{{ hexWord(way.tag) }}</span>
                      <template v-for="sr in subRowsPerLine" :key="sr">
                        <span class="simulation-cache-cell">{{ subRowOffsetLabel(sr - 1) }}</span>
                        <span
                          v-for="wj in WORDS_PER_SUBROW"
                          :key="wj"
                          class="simulation-cache-cell simulation-cache-cell-word"
                          :class="{ 'simulation-cache-group-start': isGroupStart(wj - 1), 'simulation-cache-cell-empty': !wordExistsAt(sr - 1, wj - 1) }"
                        >{{ wordExistsAt(sr - 1, wj - 1) ? hexWord(wordAt(way, sr - 1, wj - 1)) : "-" }}</span>
                      </template>
                    </template>
                    <!-- A phantom line: it was marked dirty from the start but
                         never actually accessed. The dirty bit is real, but
                         its tag and data are never shown since the program
                         hasn't touched it yet. -->
                    <template v-else-if="way.valid">
                      <span class="simulation-cache-cell" :style="{ gridColumn: 2, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }">1</span>
                      <span class="simulation-cache-cell" :style="{ gridColumn: 3, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }">1</span>
                      <span
                        v-for="col in [...orderDashColumn, colTag, ...dashColumns]"
                        :key="col"
                        class="simulation-cache-cell simulation-cache-cell-empty"
                        :style="{ gridColumn: col, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }"
                      >-</span>
                    </template>
                    <template v-else>
                      <span class="simulation-cache-cell" :style="{ gridColumn: 2, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }">0</span>
                      <span
                        v-for="col in [3, ...orderDashColumn, colTag, ...dashColumns]"
                        :key="col"
                        class="simulation-cache-cell simulation-cache-cell-empty"
                        :style="{ gridColumn: col, gridRow: `${wayRowStart(wi)} / span ${subRowsPerLine}` }"
                      >-</span>
                    </template>
                  </template>
                </template>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
/* .simulation-cache-outer handles horizontal scrolling only. Vertical
   scrolling is handled separately by .simulation-cache-body below, on
   purpose. We used to have one scroll area with a header that stayed stuck
   to the top while scrolling, but with a grid this tall (many sets), that
   caused a visible flicker as the browser kept recalculating the header's
   position. Keeping the header out of the scrolling area entirely avoids
   that problem completely. RamView.vue has the exact same setup for the
   exact same reason. */
.simulation-cache-level-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
}

.simulation-cache-jump-bar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  border-bottom: 1px solid var(--secondary);
  font-family: sans-serif;
}

.simulation-cache-jump-bar button {
  padding: 0.3rem 0.7rem;
  border-radius: 6px;
  border: 1px solid var(--secondary);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 0.8rem;
  cursor: pointer;
}

.simulation-cache-jump-bar button:hover {
  border-color: var(--accent);
}

.simulation-cache-jump-input {
  flex: 0 1 7rem;
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

.simulation-cache-jump-input-invalid {
  border-color: var(--diagram-miss-color);
}

.simulation-cache-outer {
  flex: 1 1 auto;
  min-height: 0;
  padding: 0.5rem;
  font-size: 0.75rem;
  font-family: monospace;
  overflow-x: auto;
  overflow-y: hidden;
  box-sizing: border-box;
}

.simulation-cache-grid-wrapper {
  display: flex;
  flex-direction: column;
  width: fit-content;
  height: 100%;
  margin: 0 auto;
}

.simulation-cache-grid-header {
  align-items: stretch;
  flex: 0 0 auto;
  background: var(--background);
  box-shadow: 0 2px 4px -2px rgba(0, 0, 0, 0.4);
  width: fit-content;
}

.simulation-cache-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.simulation-cache-set-block {
  align-items: stretch;
  width: fit-content;
}

.simulation-cache-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
}

.simulation-cache-cell-header {
  font-weight: 600;
  opacity: 0.75;
}

.simulation-cache-cell-word {
  padding: 0.1rem 0.3rem;
}

.simulation-cache-group-start {
  border-left: 2px dashed var(--secondary);
}

.simulation-cache-set-index {
  font-weight: 600;
}

.simulation-cache-cell-empty {
  color: var(--text-muted, var(--text));
  opacity: 0.4;
}
</style>
