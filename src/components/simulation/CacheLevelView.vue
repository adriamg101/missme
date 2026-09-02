<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { computeVirtualRange } from "../../lib/virtualScroll";
import { touchAndPrune } from "../../lib/lruCache";

const props = defineProps<{ levelId: string; side: "unified" | "i" | "d" }>();
const hierarchyStore = useHierarchyConfigStore();

const levelIndex = computed(() => hierarchyStore.hierarchy.findIndex((l) => l.id === props.levelId));

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
interface CacheLevelInfo {
  associativity: number;
  blockSize: number;
  numSets: number;
  wordsPerLine: number;
}

const levelInfo = ref<CacheLevelInfo | null>(null);
const MAX_CACHED_SETS = 4000;

const ROW_HEIGHT = 26;
const containerRef = ref<HTMLDivElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);

// Each set takes up one row per way, stacked on top of each other rather
// than side by side. Since associativity doesn't change within a level,
// every set's block is the same height, so we can treat "one set" as a
// single unit of a fixed size, the same way we'd treat "one row".
const setBlockHeight = computed(() => Math.max(1, levelInfo.value?.associativity ?? 1) * ROW_HEIGHT);

// The number of sets can be enormous (up into the billions of billions for
// a large, low-associativity cache), far too many to show as one
// continuous scrollbar (the same problem RamView.vue solves with
// WINDOW_ROWS). We use the same fix here: show only a smaller window of
// sets, at most MAX_WINDOW_HEIGHT_PX worth, anchored at `baseSet`, and
// quietly move that window as the user scrolls near either edge instead of
// trying to represent the whole cache at once.
const MAX_WINDOW_HEIGHT_PX = 520_000;
const windowSets = computed(() => {
  const capped = Math.max(100, Math.floor(MAX_WINDOW_HEIGHT_PX / setBlockHeight.value));
  return Math.min(capped, levelInfo.value?.numSets ?? capped);
});
const RECENTER_MARGIN_FRACTION = 0.025;
const recenterMarginSets = computed(() => Math.max(5, Math.floor(windowSets.value * RECENTER_MARGIN_FRACTION)));

const baseSet = ref(0);

// Sets are stored by their real index, not their position within the
// window, so cached sets stay valid even after the window moves, the same
// reasoning as RamView.vue's block cache. touchAndPrune keeps memory from
// growing without bound as the window slides.
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

// When we move the scroll position in code, the browser fires its own
// scroll event for it a moment later. Without this flag, that extra event
// would trigger our scroll handling again and could make the view jitter
// back and forth near an edge.
let ignoreNextScrollEvent = false;

function setContainerScrollTop(value: number) {
  scrollTop.value = value;
  if (containerRef.value) {
    ignoreNextScrollEvent = true;
    containerRef.value.scrollTop = value;
  }
}

// Moves the window so the set that's currently at the top of the screen
// ends up roughly in the middle instead, leaving equal room to keep
// scrolling either way. Never moves the window below set 0.
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

// Smooth trackpad scrolling (or certain screens) can leave the scroll
// position sitting between whole pixels, even though every set is placed
// at a whole-pixel position. When that happens, the browser rounds each
// cell's border slightly differently, and you see thin flickering gaps
// between cells while scrolling settles. Once scrolling stops, we snap
// back to a whole pixel so there's nothing left to round inconsistently.
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

// Jumps straight to a set, centering it in the window with equal room to
// scroll either direction from there, the same idea as RamView.vue's
// jumpToAddress, just counting sets instead of bytes.
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

// A word is always 4 bytes in this simulator, no matter the bus width, so a
// word column's byte offset is always wordIndex * 4. The processor's own
// natural word size (4 bytes on RV32I, 8 bytes on RV64I) is expressed here
// as how many word columns should be grouped together.
const WORD_SIZE = 4;
const wordsPerGroup = computed(() => (hierarchyStore.isa === "RV32I" ? 1 : 2));

// Words and the Tag are always 4 bytes, even if the value stored is small
// (say, only the lowest byte was ever written), so we pad them to 8 hex
// digits to make that clear. Addresses, on the other hand, don't have a
// fixed width and are left unpadded.
function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

function offsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * WORD_SIZE).toString(16).toUpperCase().padStart(2, "0")}`;
}

// Only matters when a group covers more than one word column (RV64I): adds
// a dashed left border marking where the next group starts.
function isGroupStart(wordIndex: number): boolean {
  return wordsPerGroup.value > 1 && wordIndex > 0 && wordIndex % wordsPerGroup.value === 0;
}

// The header and every set block share the same column layout: Set, then
// D/LRU/Tag/words. A set's ways are stacked as extra rows sharing these
// same columns rather than sitting side by side (Set spans all those rows,
// see setGridStyle below). Tag is padded to 8 hex digits (see hexWord),
// same as words, so its column can just have a fixed width instead of
// needing to measure the content.
const wayColumnCount = computed(() => 3 + (levelInfo.value?.wordsPerLine ?? 0));
const columnsStyle = computed(
  () =>
    `4rem 2.5rem 2.5rem calc(10ch + 0.6rem) repeat(${levelInfo.value?.wordsPerLine ?? 0}, minmax(calc(10ch + 0.6rem), auto))`,
);
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

// Resets the window back to the top of the level. Used when the view first
// loads and whenever the level or side changes, since a previous window's
// position and cached sets don't mean anything for a different level.
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
    // .simulation-cache-body only appears in the page once levelInfo has a
    // value, so the reference to it isn't ready right after we just set
    // levelInfo above. We wait one tick for the page to catch up before
    // measuring it, otherwise we'd always measure 0 and only show a
    // handful of rows no matter how tall the card actually is.
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
    // Only what's currently on screen could have changed, so there's no
    // need to reload the level's overall info (associativity, block size,
    // and number of sets never change while a simulation is running).
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
            <span class="simulation-cache-cell simulation-cache-cell-header">D</span>
            <span class="simulation-cache-cell simulation-cache-cell-header">LRU</span>
            <span class="simulation-cache-cell simulation-cache-cell-header">Tag</span>
            <span
              v-for="wi in levelInfo.wordsPerLine"
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
                    every way's row in this block, taking up the first column
                    for the whole block. Every cell after that is left to fall
                    into place on its own: the grid naturally fills them in
                    left-to-right, top-to-bottom, skipping the column the Set
                    number already fills, so way 0's cells fill the rest of row
                    1, way 1's fill row 2, and so on. This only works because
                    each way has exactly the right number of cells to fill out
                    one row, so nothing spills over into the next one.
                  -->
                  <span class="simulation-cache-cell simulation-cache-set-index" :style="{ gridColumn: 1, gridRow: `1 / span ${sets.get(setIndex)!.ways.length}` }">{{ setIndex }}</span>
                  <template v-for="(way, wi) in sets.get(setIndex)!.ways" :key="wi">
                    <!-- A real line: one that an actual access put there. -->
                    <template v-if="way.valid && way.recencyRank !== null">
                      <span class="simulation-cache-cell">{{ way.dirty ? "1" : "0" }}</span>
                      <span class="simulation-cache-cell">{{ way.recencyRank }}</span>
                      <span class="simulation-cache-cell simulation-cache-cell-word">{{ hexWord(way.tag) }}</span>
                      <span
                        v-for="(word, wj) in way.words"
                        :key="wj"
                        class="simulation-cache-cell simulation-cache-cell-word"
                        :class="{ 'simulation-cache-group-start': isGroupStart(wj) }"
                      >{{ hexWord(word) }}</span>
                    </template>
                    <!-- A phantom line: it was marked dirty from the start but
                         never actually accessed. The dirty bit is real, but
                         its tag and data are never shown since the program
                         hasn't touched it yet. -->
                    <template v-else-if="way.valid">
                      <span class="simulation-cache-cell">1</span>
                      <span v-for="n in wayColumnCount - 1" :key="n" class="simulation-cache-cell simulation-cache-cell-empty">-</span>
                    </template>
                    <template v-else>
                      <span v-for="n in wayColumnCount" :key="n" class="simulation-cache-cell simulation-cache-cell-empty">-</span>
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
