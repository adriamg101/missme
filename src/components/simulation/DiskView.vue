<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { useCompileConfigStore } from "../../stores/compileConfig";
import { useVirtualizedWindow } from "../../lib/virtualizedWindow";
import { touchAndPrune } from "../../lib/lruCache";

const store = useHierarchyConfigStore();
const compileConfigStore = useCompileConfigStore();

// The disk scrolls in fixed size blocks, just like RAM, a page just spans several of them.
const BLOCK_SIZE = 32;
const ROW_HEIGHT = 26;
const WINDOW_ROWS = 20_000;
const RECENTER_MARGIN_ROWS = 500;
const MAX_CACHED_PAGES = 4000;
const gridColumnsStyle = "8rem 8rem repeat(8, minmax(calc(10ch + 0.6rem), auto))";

interface DiskPage {
  address: number; // this actually holds the swap page number, not a byte address
  words: number[];
  populated: boolean;
  dirty: boolean;
}

// Keyed by page number, since the backend hands back one whole page at a time.
const pages = reactive(new Map<number, DiskPage>());

function blockAddress(blockIndex: number): number {
  return blockIndex * BLOCK_SIZE;
}

function absoluteBlock(address: number): number {
  return Math.floor(address / BLOCK_SIZE);
}

function vpnOfBlock(blockIndex: number): number {
  return Math.floor(blockAddress(blockIndex) / Math.max(1, store.pageSize));
}

// Groups the visible blocks by the page they belong to, so the page number is only shown once per group.
interface BlockGroup {
  vpn: number;
  blocks: number[];
  // How many blocks come before this group in the currently visible window.
  offsetInWindow: number;
}

const visibleGroups = computed<BlockGroup[]>(() => {
  const groups: BlockGroup[] = [];
  win.visibleIndices.value.forEach((blockIndex, i) => {
    const vpn = vpnOfBlock(blockIndex);
    const last = groups[groups.length - 1];
    if (last && last.vpn === vpn) last.blocks.push(blockIndex);
    else groups.push({ vpn, blocks: [blockIndex], offsetInWindow: i });
  });
  return groups;
});

async function fetchPage(startBlock: number, count: number) {
  const pageSize = Math.max(1, store.pageSize);
  const startAddress = blockAddress(startBlock);
  const endAddress = blockAddress(startBlock + count);
  const startVpn = Math.floor(startAddress / pageSize);
  const endVpn = Math.ceil(endAddress / pageSize);
  const vpnCount = Math.max(1, endVpn - startVpn);
  try {
    const result = await invoke<DiskPage[]>("get_disk_pages", { startVpn, count: vpnCount });
    const touched: number[] = [];
    result.forEach((p) => {
      pages.set(p.address, p);
      touched.push(p.address);
    });
    touchAndPrune(pages, touched, MAX_CACHED_PAGES);
  } catch (error) {
    console.error("Failed to fetch disk pages:", error);
  }
}

// Disk covers the whole address space, the same way RAM does when virtual memory is off.
const win = useVirtualizedWindow({
  rowHeight: ROW_HEIGHT,
  windowRows: WINDOW_ROWS,
  recenterMarginRows: RECENTER_MARGIN_ROWS,
  // Treated as unbounded so jumping to a distant address always works.
  totalRows: () => Number.MAX_SAFE_INTEGER,
  fetchPage,
});

interface BlockView {
  page: DiskPage | undefined;
  words: number[];
}

function blockView(blockIndex: number): BlockView {
  const pageSize = Math.max(1, store.pageSize);
  const address = blockAddress(blockIndex);
  const vpn = Math.floor(address / pageSize);
  const page = pages.get(vpn);
  const wordOffset = Math.floor((address % pageSize) / 4);
  const words = page ? Array.from({ length: 8 }, (_, i) => page.words[wordOffset + i] ?? 0) : [];
  return { page, words };
}

function hexAddress(address: number): string {
  return `0x${(address >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

function hexPage(vpn: number): string {
  return `0x${(vpn >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(8, "0")}`;
}

function offsetLabel(wordIndex: number): string {
  return `0x${(wordIndex * 4).toString(16).toUpperCase().padStart(2, "0")}`;
}

function jumpToAddress(address: number) {
  win.jumpTo(absoluteBlock(Math.max(0, address)));
}

function jumpToText() {
  jumpToAddress(compileConfigStore.textBase);
}

function jumpToData() {
  jumpToAddress(compileConfigStore.dataBase);
}

const pageInput = ref("");
const pageInputInvalid = ref(false);

function jumpToInputPage() {
  const trimmed = pageInput.value.trim();
  if (!/^\d+$/.test(trimmed)) {
    pageInputInvalid.value = true;
    return;
  }
  pageInputInvalid.value = false;
  const vpn = Number(trimmed);
  win.jumpTo(absoluteBlock(vpn * Math.max(1, store.pageSize)));
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  await nextTick();
  win.measureViewport();
  await fetchPage(0, Math.max(0, win.range.value.end - win.range.value.start));
  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "disk_access") return;
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
  <div class="simulation-disk-view">
    <div class="simulation-disk-jump-bar">
      <button type="button" title="Jump to .text" @click="jumpToText">.text</button>
      <button type="button" title="Jump to .data" @click="jumpToData">.data</button>
      <input
        type="text"
        class="simulation-disk-jump-input"
        :class="{ 'simulation-disk-jump-input-invalid': pageInputInvalid }"
        placeholder="Page #"
        aria-label="Jump to page"
        v-model="pageInput"
        @keydown.enter="jumpToInputPage"
        @input="pageInputInvalid = false"
      />
      <button type="button" title="Jump to this page" @click="jumpToInputPage">Go</button>
    </div>
    <div class="simulation-disk-outer">
      <div class="simulation-disk-grid-wrapper">
        <div class="simulation-disk-grid-header" :style="{ display: 'grid', gridTemplateColumns: gridColumnsStyle }">
          <span class="simulation-disk-cell simulation-disk-cell-header">VPN</span>
          <span class="simulation-disk-cell simulation-disk-cell-header">Address</span>
          <span v-for="wi in 8" :key="wi" class="simulation-disk-cell simulation-disk-cell-header">{{ offsetLabel(wi - 1) }}</span>
        </div>
        <div class="simulation-disk-body" :ref="(el) => (win.containerRef.value = el as HTMLDivElement | null)" @scroll="win.onScroll">
          <div class="simulation-disk-spacer" :style="{ height: `${win.range.value.totalHeight}px`, position: 'relative' }">
            <div
              v-for="group in visibleGroups"
              :key="group.blocks[0]"
              class="simulation-disk-group"
              :style="{
                position: 'absolute',
                top: `${(win.range.value.start + group.offsetInWindow) * ROW_HEIGHT}px`,
                height: `${group.blocks.length * ROW_HEIGHT}px`,
                display: 'grid',
                gridTemplateColumns: gridColumnsStyle,
                gridAutoRows: `${ROW_HEIGHT}px`,
              }"
            >
              <span class="simulation-disk-cell simulation-disk-vpn" :style="{ gridColumn: 1, gridRow: `1 / span ${group.blocks.length}` }">{{ hexPage(group.vpn) }}</span>
              <template v-for="blockIndex in group.blocks" :key="blockIndex">
                <template v-if="blockView(blockIndex).page">
                  <span class="simulation-disk-cell simulation-disk-address">{{ hexAddress(blockAddress(blockIndex)) }}</span>
                  <span
                    v-for="(word, wi) in blockView(blockIndex).words"
                    :key="wi"
                    class="simulation-disk-cell"
                    :class="{ 'simulation-disk-cell-empty': !blockView(blockIndex).page!.populated }"
                  >{{ blockView(blockIndex).page!.populated ? hexWord(word) : "-" }}</span>
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
.simulation-disk-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
  color: var(--text);
  transition: color var(--theme-transition-duration) ease;
}
.simulation-disk-jump-bar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem;
  border-bottom: 1px solid var(--secondary);
  transition: border-color var(--theme-transition-duration) ease;
}
.simulation-disk-jump-bar button {
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
.simulation-disk-jump-bar button:hover {
  border-color: var(--accent);
}
.simulation-disk-jump-input {
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
  transition: border-color var(--theme-transition-duration) ease, background-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}
.simulation-disk-jump-input-invalid {
  border-color: var(--diagram-miss-color);
}
.simulation-disk-outer {
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
.simulation-disk-grid-wrapper {
  display: flex;
  flex-direction: column;
  width: fit-content;
  height: 100%;
  margin: 0 auto;
  transition: none;
}
.simulation-disk-grid-header {
  align-items: stretch;
  flex: 0 0 auto;
  background: var(--background);
  box-shadow: 0 2px 4px -2px rgba(0, 0, 0, 0.4);
  width: fit-content;
  transition: background-color var(--theme-transition-duration) ease;
}
.simulation-disk-group {
  align-items: stretch;
  width: fit-content;
  transition: none;
}
.simulation-disk-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
  transition: border-color var(--theme-transition-duration) ease;
}
.simulation-disk-cell-header {
  font-weight: 600;
  opacity: 0.75;
}
.simulation-disk-vpn {
  font-weight: 600;
}
.simulation-disk-address {
  color: var(--text-muted, var(--text));
  transition: border-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}
.simulation-disk-cell-empty {
  color: var(--text-muted, var(--text));
  opacity: 0.4;
  transition: border-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}
.simulation-disk-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  transition: none;
}
</style>
