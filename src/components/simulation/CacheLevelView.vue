<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useVirtualizedWindow } from "../../lib/virtualizedWindow";
import { touchAndPrune } from "../../lib/lruCache";
import type { ReplacementPolicy } from "../../lib/hierarchy/model";

const props = defineProps<{ levelId: string; side: "unified" | "i" | "d" }>();
const hierarchyStore = useHierarchyConfigStore();

const levelIndex = computed(() => hierarchyStore.hierarchy.findIndex((l) => l.id === props.levelId));

const activePolicy = computed<ReplacementPolicy | null>(() => {
  const level = hierarchyStore.hierarchy[levelIndex.value];
  if (!level) return null;
  const entry = props.side === "unified" ? level.unified : props.side === "i" ? level.i : level.d;
  return entry?.config.replacement ?? null;
});

const orderColumnLabel = computed(() => {
  if (activePolicy.value === "FIFO") return "FIFO";
  if (activePolicy.value === "LFU") return "LFU";
  return "LRU";
});
const showOrderColumn = computed(() => activePolicy.value !== "Random");

interface CacheLineRow {
  setIndex: number;
  wayIndex: number;
  subrowIndex: number;
  valid: boolean;
  dirty: boolean;
  tag: number;
  isPhantom: boolean;
  orderValue: number | null;
  words: number[];
}
interface CacheLevelInfo {
  associativity: number;
  blockSize: number;
  numSets: number;
  wordsPerLine: number;
}

const levelInfo = ref<CacheLevelInfo | null>(null);
const MAX_CACHED_LINES = 4000;

const ROW_HEIGHT = 26;
const WINDOW_ROWS = 1000;
const RECENTER_MARGIN_ROWS = 25;
const WORDS_PER_SUBROW = 8;

const linesPerWay = computed(() => Math.max(1, Math.ceil((levelInfo.value?.wordsPerLine ?? 1) / WORDS_PER_SUBROW)));
const linesPerSet = computed(() => linesPerWay.value * Math.max(1, levelInfo.value?.associativity ?? 1));
const totalLines = computed(() => linesPerSet.value * Math.max(1, levelInfo.value?.numSets ?? 1));

const visibleWordColumns = computed(() => Math.min(WORDS_PER_SUBROW, levelInfo.value?.wordsPerLine ?? WORDS_PER_SUBROW));
const showOffsetColumn = computed(() => linesPerWay.value > 1);

const lines = reactive(new Map<number, CacheLineRow>());

async function fetchPage(startLine: number, count: number) {
  if (levelIndex.value < 0 || !levelInfo.value) return;
  try {
    const result = await invoke<CacheLineRow[]>("get_cache_lines", {
      levelIndex: levelIndex.value,
      side: props.side,
      startLine,
      count,
    });
    const touched: number[] = [];
    result.forEach((row, i) => {
      const lineIndex = startLine + i;
      lines.set(lineIndex, row);
      touched.push(lineIndex);
    });
    touchAndPrune(lines, touched, MAX_CACHED_LINES);
  } catch (error) {
    console.error("Failed to fetch cache lines:", error);
  }
}

const win = useVirtualizedWindow({
  rowHeight: ROW_HEIGHT,
  windowRows: WINDOW_ROWS,
  recenterMarginRows: RECENTER_MARGIN_ROWS,
  totalRows: () => totalLines.value,
  fetchPage,
});

const setInput = ref("");
const setInputInvalid = ref(false);

function jumpToInputSet() {
  const trimmed = setInput.value.trim();
  if (!/^\d+$/.test(trimmed)) {
    setInputInvalid.value = true;
    return;
  }
  setInputInvalid.value = false;
  win.jumpTo(Number(trimmed) * linesPerSet.value);
}

const WORD_SIZE = 4;
const wordsPerGroup = computed(() => (hierarchyStore.isa === "RV32I" ? 1 : 2));

function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

const addressBits = computed(() => (hierarchyStore.isa === "RV32I" ? 32 : 64));
const tagDigits = computed(() => {
  const blockSize = Math.max(1, levelInfo.value?.blockSize ?? 1);
  const numSets = Math.max(1, levelInfo.value?.numSets ?? 1);
  const tagBits = addressBits.value - Math.log2(blockSize) - Math.log2(numSets);
  return Math.max(1, Math.ceil(tagBits / 4));
});
function hexTag(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(tagDigits.value, "0")}`;
}

function offsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * WORD_SIZE).toString(16).toUpperCase().padStart(2, "0")}`;
}

const lineOffsetDigits = computed(() => Math.max(1, Math.ceil(Math.log2(Math.max(1, levelInfo.value?.blockSize ?? 1)) / 4)));

function subRowOffsetLabel(subrowIndex: number): string {
  return `0x${(subrowIndex * WORDS_PER_SUBROW * WORD_SIZE).toString(16).toUpperCase().padStart(lineOffsetDigits.value, "0")}`;
}

function isGroupStart(wordIndex: number): boolean {
  return wordsPerGroup.value > 1 && wordIndex > 0 && wordIndex % wordsPerGroup.value === 0;
}

const columnsStyle = computed(() => {
  const orderTrack = showOrderColumn.value ? "2.5rem " : "";
  const offsetTrack = showOffsetColumn.value ? "4rem " : "";
  const tagTrack = `minmax(calc(${tagDigits.value + 2}ch + 0.6rem), auto)`;
  return `4rem 2.5rem 2.5rem ${orderTrack}${tagTrack} ${offsetTrack}repeat(${visibleWordColumns.value}, minmax(calc(10ch + 0.6rem), auto))`;
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

const colTag = computed(() => (showOrderColumn.value ? 5 : 4));
const colOffset = computed(() => colTag.value + 1);
const colFirstWord = computed(() => (showOffsetColumn.value ? colOffset.value + 1 : colTag.value + 1));
const dashColumns = computed(() => {
  const start = showOffsetColumn.value ? colOffset.value : colFirstWord.value;
  const count = (showOffsetColumn.value ? 1 : 0) + visibleWordColumns.value;
  return Array.from({ length: count }, (_, i) => start + i);
});
const orderDashColumn = computed(() => (showOrderColumn.value ? [4] : []));

interface LineEntry {
  lineIndex: number;
  line: CacheLineRow | undefined;
}
interface WayGroup {
  wayIndex: number;
  rowStart: number;
  entries: LineEntry[];
}
interface SetGroup {
  setIndex: number;
  offsetInWindow: number;
  totalRows: number;
  ways: WayGroup[];
}

const visibleGroups = computed<SetGroup[]>(() => {
  const groups: SetGroup[] = [];
  win.visibleIndices.value.forEach((lineIndex, i) => {
    const setIndex = Math.floor(lineIndex / linesPerSet.value);
    const withinSet = lineIndex % linesPerSet.value;
    const wayIndex = Math.floor(withinSet / linesPerWay.value);
    const entry: LineEntry = { lineIndex, line: lines.get(lineIndex) };

    let setGroup = groups[groups.length - 1];
    if (!setGroup || setGroup.setIndex !== setIndex) {
      setGroup = { setIndex, offsetInWindow: i, totalRows: 0, ways: [] };
      groups.push(setGroup);
    }
    let wayGroup = setGroup.ways[setGroup.ways.length - 1];
    if (!wayGroup || wayGroup.wayIndex !== wayIndex) {
      wayGroup = { wayIndex, rowStart: setGroup.totalRows + 1, entries: [] };
      setGroup.ways.push(wayGroup);
    }
    wayGroup.entries.push(entry);
    setGroup.totalRows += 1;
  });
  return groups;
});

let unlisten: (() => void) | null = null;
let disposed = false;

async function loadLevelInfo() {
  if (levelIndex.value < 0) {
    levelInfo.value = null;
    return;
  }
  try {
    levelInfo.value = await invoke<CacheLevelInfo>("get_cache_level_info", { levelIndex: levelIndex.value, side: props.side });
    lines.clear();
    win.resetWindow();
    await nextTick();
    win.measureViewport();
    await fetchPage(0, Math.max(0, win.range.value.end - win.range.value.start));
  } catch (error) {
    console.error("Failed to fetch cache level info:", error);
  }
}

onMounted(async () => {
  await loadLevelInfo();

  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "cache_access") return;
    if (event.payload.levelIndex !== levelIndex.value) return;
    fetchPage(win.baseIndex.value + win.range.value.start, Math.max(0, win.range.value.end - win.range.value.start));
  });
  if (disposed) stopListening();
  else unlisten = stopListening;
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
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
            <span v-if="showOffsetColumn" class="simulation-cache-cell simulation-cache-cell-header">Offset</span>
            <span
              v-for="wi in visibleWordColumns"
              :key="wi"
              class="simulation-cache-cell simulation-cache-cell-header simulation-cache-cell-word"
              :class="{ 'simulation-cache-group-start': isGroupStart(wi - 1) }"
            >{{ offsetLabel(wi - 1) }}</span>
          </div>
          <div class="simulation-cache-body" :ref="(el) => (win.containerRef.value = el as HTMLDivElement | null)" @scroll="win.onScroll">
            <div class="simulation-cache-spacer" :style="{ height: `${win.range.value.totalHeight}px`, position: 'relative' }">
              <div
                v-for="group in visibleGroups"
                :key="group.setIndex"
                class="simulation-cache-set-block"
                :style="{
                  position: 'absolute',
                  top: `${(win.range.value.start + group.offsetInWindow) * ROW_HEIGHT}px`,
                  height: `${group.totalRows * ROW_HEIGHT}px`,
                  ...setGridStyle,
                }"
              >
                <span
                  class="simulation-cache-cell simulation-cache-set-index"
                  :style="{ gridColumn: 1, gridRow: `1 / span ${group.totalRows}` }"
                >{{ group.setIndex }}</span>
                <template v-for="wayGroup in group.ways" :key="wayGroup.wayIndex">
                  <template v-if="wayGroup.entries[0].line?.valid && !wayGroup.entries[0].line?.isPhantom">
                    <span class="simulation-cache-cell" :style="{ gridColumn: 2, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }">1</span>
                    <span class="simulation-cache-cell" :style="{ gridColumn: 3, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }">{{ wayGroup.entries[0].line!.dirty ? "1" : "0" }}</span>
                    <span v-if="showOrderColumn" class="simulation-cache-cell" :style="{ gridColumn: 4, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }">{{ wayGroup.entries[0].line!.orderValue }}</span>
                    <span class="simulation-cache-cell simulation-cache-cell-word" :style="{ gridColumn: colTag, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }">{{ hexTag(wayGroup.entries[0].line!.tag) }}</span>
                    <template v-for="entry in wayGroup.entries" :key="entry.lineIndex">
                      <span v-if="showOffsetColumn" class="simulation-cache-cell">{{ subRowOffsetLabel(entry.line!.subrowIndex) }}</span>
                      <span
                        v-for="wj in visibleWordColumns"
                        :key="wj"
                        class="simulation-cache-cell simulation-cache-cell-word"
                        :class="{ 'simulation-cache-group-start': isGroupStart(wj - 1), 'simulation-cache-cell-empty': wj - 1 >= entry.line!.words.length }"
                      >{{ wj - 1 < entry.line!.words.length ? hexWord(entry.line!.words[wj - 1]) : "-" }}</span>
                    </template>
                  </template>
                  <template v-else-if="wayGroup.entries[0].line?.valid">
                    <span class="simulation-cache-cell" :style="{ gridColumn: 2, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }">1</span>
                    <span class="simulation-cache-cell" :style="{ gridColumn: 3, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }">1</span>
                    <span
                      v-for="col in [...orderDashColumn, colTag, ...dashColumns]"
                      :key="col"
                      class="simulation-cache-cell simulation-cache-cell-empty"
                      :style="{ gridColumn: col, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }"
                    >-</span>
                  </template>
                  <template v-else-if="wayGroup.entries[0].line">
                    <span class="simulation-cache-cell" :style="{ gridColumn: 2, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }">0</span>
                    <span
                      v-for="col in [3, ...orderDashColumn, colTag, ...dashColumns]"
                      :key="col"
                      class="simulation-cache-cell simulation-cache-cell-empty"
                      :style="{ gridColumn: col, gridRow: `${wayGroup.rowStart} / span ${wayGroup.entries.length}` }"
                    >-</span>
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
.simulation-cache-level-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
  color: var(--text);
  transition: color var(--theme-transition-duration) ease;
}

.simulation-cache-jump-bar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  border-bottom: 1px solid var(--secondary);
  font-family: sans-serif;
  transition: border-color var(--theme-transition-duration) ease;
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
  transition: border-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
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
  transition: border-color var(--theme-transition-duration) ease, background-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
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
  transition: none;
}

.simulation-cache-grid-wrapper {
  display: flex;
  flex-direction: column;
  width: fit-content;
  height: 100%;
  margin: 0 auto;
  transition: none;
}

.simulation-cache-grid-header {
  align-items: stretch;
  flex: 0 0 auto;
  background: var(--background);
  box-shadow: 0 2px 4px -2px rgba(0, 0, 0, 0.4);
  width: fit-content;
  transition: background-color var(--theme-transition-duration) ease;
}

.simulation-cache-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  transition: none;
}

.simulation-cache-set-block {
  align-items: stretch;
  width: fit-content;
  transition: none;
}

.simulation-cache-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
  transition: border-color var(--theme-transition-duration) ease;
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
  transition: border-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}
</style>
