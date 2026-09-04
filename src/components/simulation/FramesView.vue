<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useVirtualizedWindow } from "../../lib/virtualizedWindow";
import { touchAndPrune } from "../../lib/lruCache";

const store = useHierarchyConfigStore();
const ROW_HEIGHT = 26;
const WINDOW_ROWS = 20_000;
const RECENTER_MARGIN_ROWS = 500;
const MAX_CACHED_FRAMES = 4000;
const WORDS_PER_SUBROW = 8;

interface Frame {
  frame: number;
  vpn: number | null;
  valid: boolean;
  dirty: boolean;
  words: number[];
  populated: boolean[];
}

const frames = reactive(new Map<number, Frame>());
const numFrames = computed(() => Math.max(1, store.ram.frames));
const wordsPerFrame = computed(() => Math.max(1, Math.floor(store.pageSize / 4)));

// A page can hold far more words than fit in a readable row, so each frame is drawn as several rows.
const subRowsPerFrame = computed(() => Math.max(1, Math.ceil(wordsPerFrame.value / WORDS_PER_SUBROW)));
const frameBlockHeight = computed(() => subRowsPerFrame.value * ROW_HEIGHT);

const gridColumnsStyle = `4rem calc(10ch + 0.6rem) 2.5rem 2.5rem 4rem repeat(${WORDS_PER_SUBROW}, minmax(calc(10ch + 0.6rem), auto))`;

async function fetchPage(startFrame: number, count: number) {
  try {
    const result = await invoke<Frame[]>("get_frames", { startFrame, count });
    const touched: number[] = [];
    result.forEach((f) => {
      frames.set(f.frame, f);
      touched.push(f.frame);
    });
    touchAndPrune(frames, touched, MAX_CACHED_FRAMES);
  } catch (error) {
    console.error("Failed to fetch frames:", error);
  }
}

const win = useVirtualizedWindow({
  rowHeight: () => frameBlockHeight.value,
  windowRows: WINDOW_ROWS,
  recenterMarginRows: RECENTER_MARGIN_ROWS,
  totalRows: () => numFrames.value,
  fetchPage,
});

function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

// Which byte within a row of 8 words a given column is.
function columnOffsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * 4).toString(16).toUpperCase().padStart(2, "0")}`;
}

function subRowOffsetLabel(subRow: number): string {
  return `0x${(subRow * WORDS_PER_SUBROW * 4).toString(16).toUpperCase().padStart(4, "0")}`;
}

function wordAt(frame: Frame, subRow: number, column: number): number {
  return frame.words[subRow * WORDS_PER_SUBROW + column] ?? 0;
}

// Tells a word that was never really written apart from one that really holds a zero.
function wordPopulatedAt(frame: Frame, subRow: number, column: number): boolean {
  return frame.populated[subRow * WORDS_PER_SUBROW + column] ?? false;
}

const frameInput = ref("");
const frameInputInvalid = ref(false);

function jumpToInputFrame() {
  const trimmed = frameInput.value.trim();
  if (!/^\d+$/.test(trimmed)) {
    frameInputInvalid.value = true;
    return;
  }
  frameInputInvalid.value = false;
  win.jumpTo(Number(trimmed));
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  // Waits for the page to render before measuring the scrolling area, or it always reads zero.
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
          <span class="simulation-frames-cell simulation-frames-cell-header">Offset</span>
          <span v-for="wi in WORDS_PER_SUBROW" :key="wi" class="simulation-frames-cell simulation-frames-cell-header">{{ columnOffsetLabel(wi - 1) }}</span>
        </div>
        <div class="simulation-frames-body" :ref="(el) => (win.containerRef.value = el as HTMLDivElement | null)" @scroll="win.onScroll">
          <div class="simulation-frames-spacer" :style="{ height: `${win.range.value.totalHeight}px`, position: 'relative' }">
            <div
              v-for="(frameIndex, i) in win.visibleIndices.value"
              :key="frameIndex"
              class="simulation-frames-block"
              :style="{
                position: 'absolute',
                top: `${(win.range.value.start + i) * frameBlockHeight}px`,
                height: `${frameBlockHeight}px`,
                display: 'grid',
                gridTemplateColumns: gridColumnsStyle,
                gridAutoRows: `${ROW_HEIGHT}px`,
              }"
            >
              <template v-if="frames.get(frameIndex)">
                <span class="simulation-frames-cell simulation-frames-frame-index" :style="{ gridColumn: 1, gridRow: `1 / span ${subRowsPerFrame}` }">{{ frameIndex }}</span>
                <template v-if="frames.get(frameIndex)!.valid">
                  <span class="simulation-frames-cell" :style="{ gridColumn: 2, gridRow: `1 / span ${subRowsPerFrame}` }">{{ hexWord(frames.get(frameIndex)!.vpn!) }}</span>
                  <span class="simulation-frames-cell" :style="{ gridColumn: 3, gridRow: `1 / span ${subRowsPerFrame}` }">1</span>
                  <span class="simulation-frames-cell" :style="{ gridColumn: 4, gridRow: `1 / span ${subRowsPerFrame}` }">{{ frames.get(frameIndex)!.dirty ? "1" : "0" }}</span>
                  <template v-for="sr in subRowsPerFrame" :key="sr">
                    <span class="simulation-frames-cell">{{ subRowOffsetLabel(sr - 1) }}</span>
                    <span
                      v-for="wi in WORDS_PER_SUBROW"
                      :key="wi"
                      class="simulation-frames-cell"
                      :class="{ 'simulation-frames-cell-empty': !wordPopulatedAt(frames.get(frameIndex)!, sr - 1, wi - 1) }"
                    >{{ wordPopulatedAt(frames.get(frameIndex)!, sr - 1, wi - 1) ? hexWord(wordAt(frames.get(frameIndex)!, sr - 1, wi - 1)) : "-" }}</span>
                  </template>
                </template>
                <template v-else>
                  <span class="simulation-frames-cell simulation-frames-cell-empty" :style="{ gridColumn: 2, gridRow: `1 / span ${subRowsPerFrame}` }">-</span>
                  <span class="simulation-frames-cell simulation-frames-cell-empty" :style="{ gridColumn: 3, gridRow: `1 / span ${subRowsPerFrame}` }">-</span>
                  <span class="simulation-frames-cell simulation-frames-cell-empty" :style="{ gridColumn: 4, gridRow: `1 / span ${subRowsPerFrame}` }">-</span>
                  <template v-for="sr in subRowsPerFrame" :key="sr">
                    <span class="simulation-frames-cell">{{ subRowOffsetLabel(sr - 1) }}</span>
                    <span v-for="wi in WORDS_PER_SUBROW" :key="wi" class="simulation-frames-cell simulation-frames-cell-empty">-</span>
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
}
.simulation-frames-jump-bar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  border-bottom: 1px solid var(--secondary);
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
}
.simulation-frames-grid-wrapper {
  display: flex;
  flex-direction: column;
  width: fit-content;
  height: 100%;
  margin: 0 auto;
}
.simulation-frames-grid-header,
.simulation-frames-block {
  width: fit-content;
}
.simulation-frames-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
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
}
.simulation-frames-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}
</style>
