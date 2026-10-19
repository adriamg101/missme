<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useVirtualizedWindow } from "../../lib/virtualizedWindow";
import { touchAndPrune } from "../../lib/lruCache";

const store = useHierarchyConfigStore();
const ROW_HEIGHT = 26;
const WINDOW_ROWS = 1000;
const RECENTER_MARGIN_ROWS = 25;
const MAX_CACHED_LINES = 4000;
const WORDS_PER_SUBROW = 8;

interface FrameLineRow {
  frame: number;
  subrowIndex: number;
  vpn: number | null;
  valid: boolean;
  dirty: boolean;
  words: number[];
  populated: boolean[];
}

const lines = reactive(new Map<number, FrameLineRow>());
const numFrames = computed(() => Math.max(1, store.ram.frames));
const wordsPerFrame = computed(() => Math.max(1, Math.floor(store.pageSize / 4)));
const linesPerFrame = computed(() => Math.max(1, Math.ceil(wordsPerFrame.value / WORDS_PER_SUBROW)));
const totalLines = computed(() => linesPerFrame.value * numFrames.value);

const visibleWordColumns = computed(() => Math.min(WORDS_PER_SUBROW, wordsPerFrame.value));
const showOffsetColumn = computed(() => linesPerFrame.value > 1);

const gridColumnsStyle = computed(() => {
  const offsetTrack = showOffsetColumn.value ? "4rem " : "";
  const vpnTrack = `minmax(calc(${vpnDigits.value + 2}ch + 0.6rem), auto)`;
  return `4rem ${vpnTrack} 2.5rem 2.5rem ${offsetTrack}repeat(${visibleWordColumns.value}, minmax(calc(10ch + 0.6rem), auto))`;
});

async function fetchPage(startLine: number, count: number) {
  try {
    const result = await invoke<FrameLineRow[]>("get_frame_lines", { startLine, count });
    const touched: number[] = [];
    result.forEach((row, i) => {
      const lineIndex = startLine + i;
      lines.set(lineIndex, row);
      touched.push(lineIndex);
    });
    touchAndPrune(lines, touched, MAX_CACHED_LINES);
  } catch (error) {
    console.error("Failed to fetch frame lines:", error);
  }
}

const win = useVirtualizedWindow({
  rowHeight: ROW_HEIGHT,
  windowRows: WINDOW_ROWS,
  recenterMarginRows: RECENTER_MARGIN_ROWS,
  totalRows: () => totalLines.value,
  fetchPage,
});

function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

const addressBits = computed(() => (store.isa === "RV32I" ? 32 : 64));
const vpnDigits = computed(() => {
  const pageSize = Math.max(1, store.pageSize);
  return Math.max(1, Math.ceil((addressBits.value - Math.log2(pageSize)) / 4));
});
function hexVpn(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(vpnDigits.value, "0")}`;
}

function columnOffsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * 4).toString(16).toUpperCase().padStart(2, "0")}`;
}

const pageOffsetDigits = computed(() => Math.max(1, Math.ceil(Math.log2(Math.max(1, store.pageSize)) / 4)));

function subRowOffsetLabel(subrowIndex: number): string {
  return `0x${(subrowIndex * WORDS_PER_SUBROW * 4).toString(16).toUpperCase().padStart(pageOffsetDigits.value, "0")}`;
}

interface FrameGroupEntry {
  lineIndex: number;
  row: FrameLineRow | undefined;
}
interface FrameGroup {
  frame: number;
  entries: FrameGroupEntry[];
  offsetInWindow: number;
}

const visibleGroups = computed<FrameGroup[]>(() => {
  const groups: FrameGroup[] = [];
  win.visibleIndices.value.forEach((lineIndex, i) => {
    const frame = Math.floor(lineIndex / linesPerFrame.value);
    const last = groups[groups.length - 1];
    const entry = { lineIndex, row: lines.get(lineIndex) };
    if (last && last.frame === frame) last.entries.push(entry);
    else groups.push({ frame, entries: [entry], offsetInWindow: i });
  });
  return groups;
});

const frameInput = ref("");
const frameInputInvalid = ref(false);

function jumpToInputFrame() {
  const trimmed = frameInput.value.trim();
  if (!/^\d+$/.test(trimmed)) {
    frameInputInvalid.value = true;
    return;
  }
  frameInputInvalid.value = false;
  win.jumpTo(Number(trimmed) * linesPerFrame.value);
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  await nextTick();
  win.measureViewport();
  await fetchPage(0, Math.max(0, win.range.value.end - win.range.value.start));
  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "ram_access") return;
    fetchPage(win.baseIndex.value + win.range.value.start, Math.max(0, win.range.value.end - win.range.value.start));
  });
  if (disposed) stopListening();
  else unlisten = stopListening;
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<template>
  <div class="simulation-frames-view">
    <div class="simulation-frames-jump-bar">
      <input
        type="text"
        class="simulation-frames-jump-input"
        :class="{ 'simulation-frames-jump-input-invalid': frameInputInvalid }"
        placeholder="Frame #"
        aria-label="Jump to frame"
        v-model="frameInput"
        @keydown.enter="jumpToInputFrame"
        @input="frameInputInvalid = false"
      />
      <button type="button" title="Jump to this frame" @click="jumpToInputFrame">Go</button>
    </div>
    <div class="simulation-frames-outer">
      <div class="simulation-frames-grid-wrapper">
        <div class="simulation-frames-grid-header" :style="{ display: 'grid', gridTemplateColumns: gridColumnsStyle }">
          <span class="simulation-frames-cell simulation-frames-cell-header">Frame</span>
          <span class="simulation-frames-cell simulation-frames-cell-header">VPN</span>
          <span class="simulation-frames-cell simulation-frames-cell-header">V</span>
          <span class="simulation-frames-cell simulation-frames-cell-header">D</span>
          <span v-if="showOffsetColumn" class="simulation-frames-cell simulation-frames-cell-header">Offset</span>
          <span v-for="wi in visibleWordColumns" :key="wi" class="simulation-frames-cell simulation-frames-cell-header">{{ columnOffsetLabel(wi - 1) }}</span>
        </div>
        <div class="simulation-frames-body" :ref="(el) => (win.containerRef.value = el as HTMLDivElement | null)" @scroll="win.onScroll">
          <div class="simulation-frames-spacer" :style="{ height: `${win.range.value.totalHeight}px`, position: 'relative' }">
            <div
              v-for="group in visibleGroups"
              :key="group.entries[0].lineIndex"
              class="simulation-frames-block"
              :style="{
                position: 'absolute',
                top: `${(win.range.value.start + group.offsetInWindow) * ROW_HEIGHT}px`,
                height: `${group.entries.length * ROW_HEIGHT}px`,
                display: 'grid',
                gridTemplateColumns: gridColumnsStyle,
                gridAutoRows: `${ROW_HEIGHT}px`,
              }"
            >
              <span class="simulation-frames-cell simulation-frames-frame-index" :style="{ gridColumn: 1, gridRow: `1 / span ${group.entries.length}` }">{{ group.frame }}</span>
              <template v-if="group.entries[0].row">
                <template v-if="group.entries[0].row.valid">
                  <span class="simulation-frames-cell" :style="{ gridColumn: 2, gridRow: `1 / span ${group.entries.length}` }">{{ hexVpn(group.entries[0].row.vpn!) }}</span>
                  <span class="simulation-frames-cell" :style="{ gridColumn: 3, gridRow: `1 / span ${group.entries.length}` }">1</span>
                  <span class="simulation-frames-cell" :style="{ gridColumn: 4, gridRow: `1 / span ${group.entries.length}` }">{{ group.entries[0].row.dirty ? "1" : "0" }}</span>
                  <template v-for="entry in group.entries" :key="entry.lineIndex">
                    <span v-if="showOffsetColumn" class="simulation-frames-cell">{{ subRowOffsetLabel(entry.row!.subrowIndex) }}</span>
                    <span
                      v-for="wi in visibleWordColumns"
                      :key="wi"
                      class="simulation-frames-cell"
                      :class="{ 'simulation-frames-cell-empty': !(entry.row!.populated[wi - 1] ?? false) }"
                    >{{ entry.row!.populated[wi - 1] ? hexWord(entry.row!.words[wi - 1]) : "-" }}</span>
                  </template>
                </template>
                <template v-else>
                  <span class="simulation-frames-cell simulation-frames-cell-empty" :style="{ gridColumn: 2, gridRow: `1 / span ${group.entries.length}` }">-</span>
                  <span class="simulation-frames-cell simulation-frames-cell-empty" :style="{ gridColumn: 3, gridRow: `1 / span ${group.entries.length}` }">-</span>
                  <span class="simulation-frames-cell simulation-frames-cell-empty" :style="{ gridColumn: 4, gridRow: `1 / span ${group.entries.length}` }">-</span>
                  <template v-for="entry in group.entries" :key="entry.lineIndex">
                    <span v-if="showOffsetColumn" class="simulation-frames-cell">{{ subRowOffsetLabel(entry.row!.subrowIndex) }}</span>
                    <span v-for="wi in visibleWordColumns" :key="wi" class="simulation-frames-cell simulation-frames-cell-empty">-</span>
                  </template>
                </template>
              </template>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.simulation-frames-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
  color: var(--text);
  transition: color var(--theme-transition-duration) ease;
}
.simulation-frames-jump-bar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  border-bottom: 1px solid var(--secondary);
  transition: border-color var(--theme-transition-duration) ease;
}
.simulation-frames-jump-bar button {
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
.simulation-frames-jump-bar button:hover {
  border-color: var(--accent);
}
.simulation-frames-jump-input {
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
.simulation-frames-jump-input-invalid {
  border-color: var(--diagram-miss-color);
}
.simulation-frames-outer {
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
.simulation-frames-grid-wrapper {
  display: flex;
  flex-direction: column;
  width: fit-content;
  height: 100%;
  margin: 0 auto;
  transition: none;
}
.simulation-frames-grid-header,
.simulation-frames-block {
  width: fit-content;
  transition: none;
}
.simulation-frames-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
  transition: border-color var(--theme-transition-duration) ease;
}
.simulation-frames-cell-header {
  font-weight: 600;
  opacity: 0.75;
}
.simulation-frames-frame-index {
  font-weight: 600;
}
.simulation-frames-cell-empty {
  color: var(--text-muted, var(--text));
  opacity: 0.4;
  transition: border-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}
.simulation-frames-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  transition: none;
}
</style>
