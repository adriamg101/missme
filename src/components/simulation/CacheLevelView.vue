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

// Each set occupies one row per way, stacked vertically (not side by
// side) — associativity is constant across a level, so every set's block
// is the same height, letting the virtual-scroll math treat "one set" as
// a single uniform-height unit the same way it already treats "one row".
const setBlockHeight = computed(() => Math.max(1, levelInfo.value?.associativity ?? 1) * ROW_HEIGHT);

// numSets can be astronomically large (up to 2^63 for a big,
// low-associativity cache) — nowhere near renderable as one continuous
// scrollbar (see RamView.vue's WINDOW_ROWS for the same browser-max-
// element-height problem). So, same fix: a sliding window of at most
// MAX_WINDOW_HEIGHT_PX worth of sets, anchored at `baseSet`, re-anchoring
// near either edge as the user scrolls rather than ever trying to
// represent the whole level as one scrollbar.
const MAX_WINDOW_HEIGHT_PX = 520_000;
const windowSets = computed(() => {
  const capped = Math.max(100, Math.floor(MAX_WINDOW_HEIGHT_PX / setBlockHeight.value));
  return Math.min(capped, levelInfo.value?.numSets ?? capped);
});
const RECENTER_MARGIN_FRACTION = 0.025;
const recenterMarginSets = computed(() => Math.max(5, Math.floor(windowSets.value * RECENTER_MARGIN_FRACTION)));

const baseSet = ref(0);

// Keyed by absolute set index (not window-relative), so cached sets stay
// valid across a re-anchor — see RamView.vue's `blocks` map for the same
// reasoning. touchAndPrune's LRU eviction keeps memory bounded as the
// window slides.
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

// Writing to a DOM element's scrollTop dispatches its own native 'scroll'
// event asynchronously — without this guard, that synthetic event would
// feed back into onScroll, re-running maybeRecenter and jittering the
// window back and forth on every real scroll tick near an edge.
let ignoreNextScrollEvent = false;

function setContainerScrollTop(value: number) {
  scrollTop.value = value;
  if (containerRef.value) {
    ignoreNextScrollEvent = true;
    containerRef.value.scrollTop = value;
  }
}

// Re-anchors the window so the set currently at the top of the viewport
// ends up roughly in the middle, giving equal room to keep scrolling in
// either direction. Never anchors below set 0.
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

// A fractional scrollTop (e.g. from trackpad/smooth-scroll momentum, or a
// non-integer devicePixelRatio) puts the viewport at a non-whole-pixel
// offset while every set's own `top` is an integer — the browser then has
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

// Jumps straight to a set, centering it in the window with equal room to
// scroll either direction from there — same idea as RamView.vue's
// jumpToAddress, just in "set index" units instead of bytes.
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

// Word size is fixed at 4 bytes across this engine, regardless of bus width
// (see the S4 plan's rulings) — so a word-column's byte offset is always
// wordIndex * 4, and the architecture's native word size (4 on RV32I, 8 on
// RV64I) is expressed as a number of word-columns to group together.
const WORD_SIZE = 4;
const wordsPerGroup = computed(() => (hierarchyStore.isa === "RV32I" ? 1 : 2));

// Words and Tag are always the full 4-byte range even when the stored
// value is small (e.g. only the low byte was ever written) — zero-pad to 8
// hex digits so that stays visually obvious, unlike addresses which don't
// have a fixed width and are left unpadded.
function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

function offsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * WORD_SIZE).toString(16).toUpperCase().padStart(2, "0")}`;
}

// Only meaningful when a group spans more than one word-column (RV64I) — a
// dashed left border marking where the next native-word group starts.
function isGroupStart(wordIndex: number): boolean {
  return wordsPerGroup.value > 1 && wordIndex > 0 && wordIndex % wordsPerGroup.value === 0;
}

// One shared column layout for the header AND every set block: Set, then
// D/LRU/Tag/words — a set's ways no longer sit side by side in their own
// sub-grid, they stack as additional rows sharing these same columns
// (Set spans all of them via grid-row, see setGridStyle below). Tag is
// zero-padded to 8 hex digits (see hexWord), same as words, so its column
// can just be a fixed width instead of measuring content.
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

// Resets the window back to the top of the level — used on mount and
// whenever the level/side changes, since a previous window's baseSet/
// cached sets are meaningless for a different level.
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
    // .simulation-cache-body only exists once levelInfo is truthy (it's
    // behind a v-if) — containerRef is still null at the point levelInfo
    // was just assigned above, so measuring before this tick would always
    // read 0, capping the visible window to just the overscan rows
    // regardless of how tall the card actually is.
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
    // Only the currently-visible window can have changed — no need to
    // re-fetch level info (associativity/blockSize/numSets are constant
    // for the lifetime of a running simulation).
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
                    Set is placed first and spans every way-row of this block via
                    grid-row, occupying column 1 for the whole block. Every
                    subsequent cell below is left unpositioned (no explicit
                    grid-row/grid-column) — CSS Grid's default sparse auto-flow
                    places them left-to-right, top-to-bottom, automatically
                    skipping the column Set already occupies, so way 0's cells
                    fill row 1's remaining columns, way 1's fill row 2's, and so
                    on. This only works because each way contributes exactly
                    wayColumnCount cells — matching the columns available per row
                    once Set's column is excluded — so every way lands on its own
                    row with nothing left over to spill into the next one.
                  -->
                  <span class="simulation-cache-cell simulation-cache-set-index" :style="{ gridColumn: 1, gridRow: `1 / span ${sets.get(setIndex)!.ways.length}` }">{{ setIndex }}</span>
                  <template v-for="(way, wi) in sets.get(setIndex)!.ways" :key="wi">
                    <!-- A real line: one an actual access put there. -->
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
                    <!-- A phantom (dirty-seeded but never really accessed)
                         line: the dirty bit is real, but its tag/data are
                         never shown — the program hasn't touched it yet. -->
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
/* .simulation-cache-outer handles horizontal scroll only — vertical is
   handled by .simulation-cache-body below, deliberately NOT combined into
   one scroll container. A single scroller with a `position: sticky` header
   was the previous approach, but browsers recompute a sticky element's
   position every scroll frame, and at the heights this grid can reach with
   many sets, that recomputation is prone to visible 1px rounding jitter.
   Splitting the header out into normal (non-sticky, non-scrolling-
   vertically) flow avoids that class of bug entirely, not just mitigates
   it — see RamView.vue, which had the exact same issue with the exact
   same fix. */
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
