<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useVirtualizedWindow } from "../../lib/virtualizedWindow";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import IconButton from "../ui/IconButton.vue";

const hierarchyStore = useHierarchyConfigStore();

interface PteRow {
  level: number;
  index: number;
  key: number;
  pointsToOrPpn: number;
  present: boolean;
}

interface NavStep {
  level: number;
  parentKey: number | null;
}

const ROW_HEIGHT = 26;
const WINDOW_ROWS = 1000;
const RECENTER_MARGIN_ROWS = 25;

// Level 0 is the leaf, matching how the backend counts levels.
const levelBits = ref<number[]>([]);
const rootLevel = ref(1);
const stack = ref<NavStep[]>([]);
const currentStep = computed<NavStep | undefined>(() => stack.value[stack.value.length - 1]);
const isLeaf = computed(() => (currentStep.value?.level ?? 1) === 1);

// Only the table currently on screen is kept here, moving to another table clears it.
const entries = reactive(new Map<number, PteRow>());

const entryCount = ref(0);

async function fetchEntryCount() {
  const step = currentStep.value;
  if (!step) {
    entryCount.value = 0;
    return;
  }
  try {
    entryCount.value = await invoke<number>("get_page_table_entry_count", { level: step.level, parentKey: step.parentKey });
  } catch (error) {
    console.error("Failed to fetch page table entry count:", error);
    entryCount.value = 0;
  }
}

const ppnDigits = computed(() => {
  const numFrames = Math.max(1, hierarchyStore.ram.frames);
  return Math.max(1, Math.ceil(Math.log2(numFrames) / 4));
});
function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(ppnDigits.value, "0")}`;
}

const indexDigits = computed(() => {
  const level = currentStep.value?.level ?? 1;
  const bits = (levelBits.value ?? [])[level - 1] ?? 0;
  return Math.max(1, Math.ceil(bits / 4));
});
function hexIndex(index: number): string {
  return `0x${(index >>> 0).toString(16).toUpperCase().padStart(indexDigits.value, "0")}`;
}

const gridColumnsStyle = computed(() => {
  const indexTrack = `minmax(calc(${indexDigits.value + 2}ch + 0.6rem), auto)`;
  const ppnTrack = `minmax(calc(${ppnDigits.value + 2}ch + 0.6rem), auto)`;
  return `${indexTrack} ${ppnTrack} 2.5rem 2.5rem`;
});

async function fetchPage(startIndex: number, count: number) {
  const step = currentStep.value;
  if (!step) return;
  try {
    const rows = await invoke<PteRow[]>("get_page_table_entries", {
      level: step.level,
      parentKey: step.parentKey,
      startIndex,
      count,
    });
    rows.forEach((row, i) => entries.set(startIndex + i, row));
  } catch (error) {
    console.error("Failed to fetch page table entries:", error);
  }
}

const win = useVirtualizedWindow({
  rowHeight: ROW_HEIGHT,
  windowRows: WINDOW_ROWS,
  recenterMarginRows: RECENTER_MARGIN_ROWS,
  totalRows: () => entryCount.value,
  fetchPage,
});

async function refreshCurrentTable() {
  entries.clear();
  await fetchEntryCount();
  win.resetWindow();
  await nextTick();
  win.measureViewport();
  await fetchPage(win.range.value.start, Math.max(0, win.range.value.end - win.range.value.start));
}

async function goToRoot() {
  stack.value = [{ level: rootLevel.value, parentKey: null }];
  await refreshCurrentTable();
}

async function goToChild(row: PteRow | undefined) {
  if (!row?.present || !currentStep.value) return;
  stack.value.push({ level: currentStep.value.level - 1, parentKey: row.key });
  await refreshCurrentTable();
}

async function goToParent() {
  if (stack.value.length <= 1) return;
  stack.value.pop();
  await refreshCurrentTable();
}

const indexInput = ref("");
const indexInputInvalid = ref(false);

function jumpToInputIndex() {
  const trimmed = indexInput.value.trim();
  if (!/^\d+$/.test(trimmed)) {
    indexInputInvalid.value = true;
    return;
  }
  indexInputInvalid.value = false;
  win.jumpTo(Number(trimmed));
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  try {
    rootLevel.value = await invoke<number>("get_page_table_levels", {});
    levelBits.value = await invoke<number[]>("get_page_table_level_bits", {});
  } catch (error) {
    console.error("Failed to fetch page table structure:", error);
  }
  await goToRoot();
  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    const kind = event.payload.kind;
    if (kind !== "tlb_access" && kind !== "ram_access" && kind !== "disk_access") return;
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
  <div class="simulation-page-table-view">
    <div class="simulation-page-table-toolbar">
      <div class="simulation-page-table-toolbar-left">
        <IconButton v-if="stack.length > 1" size="sm" ariaLabel="Back to parent table" @click="goToParent">
          <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M13 8H3M7 4 3 8l4 4" />
          </svg>
        </IconButton>
        <input
          type="text"
          class="simulation-page-table-jump-input"
          :class="{ 'simulation-page-table-jump-input-invalid': indexInputInvalid }"
          placeholder="Index"
          aria-label="Jump to index"
          v-model="indexInput"
          @keydown.enter="jumpToInputIndex"
          @input="indexInputInvalid = false"
        />
        <button type="button" class="simulation-page-table-go-button" title="Jump to this index" @click="jumpToInputIndex">Go</button>
      </div>
      <div class="simulation-page-table-title">Level {{ currentStep?.level ?? "" }}</div>
      <div class="simulation-page-table-toolbar-right"></div>
    </div>
    <div class="simulation-page-table-header" :style="{ gridTemplateColumns: gridColumnsStyle }">
      <span class="simulation-page-table-cell simulation-page-table-cell-header">Index</span>
      <span class="simulation-page-table-cell simulation-page-table-cell-header">PPN</span>
      <span class="simulation-page-table-cell simulation-page-table-cell-header">P</span>
      <span v-if="!isLeaf" class="simulation-page-table-cell simulation-page-table-cell-header"></span>
    </div>
    <div class="simulation-page-table-body" :ref="(el) => (win.containerRef.value = el as HTMLDivElement | null)" @scroll="win.onScroll">
      <div class="simulation-page-table-spacer" :style="{ height: `${win.range.value.totalHeight}px`, position: 'relative' }">
        <div
          v-for="(index, i) in win.visibleIndices.value"
          :key="index"
          class="simulation-page-table-row"
          :style="{ position: 'absolute', top: `${(win.range.value.start + i) * ROW_HEIGHT}px`, height: `${ROW_HEIGHT}px`, gridTemplateColumns: gridColumnsStyle }"
        >
          <template v-if="entries.get(index)">
            <span class="simulation-page-table-cell">{{ hexIndex(entries.get(index)!.index) }}</span>
            <span v-if="isLeaf" class="simulation-page-table-cell">{{ hexWord(entries.get(index)!.pointsToOrPpn) }}</span>
            <span v-else class="simulation-page-table-cell simulation-page-table-cell-empty">-</span>
            <span class="simulation-page-table-cell">{{ entries.get(index)!.present ? "1" : "0" }}</span>
            <span v-if="!isLeaf" class="simulation-page-table-cell">
              <IconButton v-if="entries.get(index)!.present" size="sm" ariaLabel="Open this table" @click="goToChild(entries.get(index)!)">
                <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M3 8h10M9 4l4 4-4 4" />
                </svg>
              </IconButton>
            </span>
          </template>
          <template v-else>
            <span class="simulation-page-table-cell simulation-page-table-cell-empty">-</span>
            <span class="simulation-page-table-cell simulation-page-table-cell-empty">-</span>
            <span class="simulation-page-table-cell simulation-page-table-cell-empty">-</span>
            <span v-if="!isLeaf" class="simulation-page-table-cell"></span>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.simulation-page-table-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 0.5rem;
  box-sizing: border-box;
  color: var(--text);
  transition: color var(--theme-transition-duration) ease;
  font-size: 0.75rem;
  font-family: monospace;
}
.simulation-page-table-title {
  font-weight: 600;
  opacity: 0.85;
  text-align: center;
  white-space: nowrap;
  transition: none;
}
.simulation-page-table-toolbar {
  flex: 0 0 auto;
  display: grid;
  /* The two outer columns stay equal width no matter how wide the left
     controls are, so the title in the middle column stays centered on the
     whole row (the card), not just centered between the other elements. */
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  padding-bottom: 0.5rem;
  transition: none;
}
.simulation-page-table-toolbar-left {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  justify-self: start;
  transition: none;
}
.simulation-page-table-toolbar button:hover {
  border-color: var(--accent);
}
.simulation-page-table-jump-input {
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
.simulation-page-table-jump-input-invalid {
  border-color: var(--diagram-miss-color);
}
.simulation-page-table-go-button {
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
.simulation-page-table-header,
.simulation-page-table-row {
  display: grid;
  width: fit-content;
  margin: 0 auto;
  transition: none;
}
.simulation-page-table-row {
  /* The row is absolutely positioned (see the template's :style), so
     "margin: 0 auto" alone can't center it - it also needs both left and
     right anchored to the spacer for the auto margins to have any span
     to distribute across. */
  left: 0;
  right: 0;
}
.simulation-page-table-header {
  flex: 0 0 auto;
  background: var(--background);
  box-shadow: 0 2px 4px -2px rgba(0, 0, 0, 0.4);
  transition: background-color var(--theme-transition-duration) ease;
}
.simulation-page-table-header .simulation-page-table-cell {
  border-top: 1px solid var(--secondary);
}
.simulation-page-table-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  /* No top border here on purpose: adjacent rows are separate elements, so
     one row's bottom border and the next row's top border would otherwise
     stack into a faint double line. Each row's bottom border alone is the
     divider; the header's own top border below closes off the top. */
  border-left: 1px solid var(--secondary);
  border-right: 1px solid var(--secondary);
  border-bottom: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
  transition: border-color var(--theme-transition-duration) ease;
}
.simulation-page-table-cell-header {
  font-weight: 600;
  opacity: 0.75;
}
.simulation-page-table-cell-empty {
  color: var(--text-muted, var(--text));
  opacity: 0.4;
  transition: border-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}
.simulation-page-table-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  transition: none;
}
</style>
