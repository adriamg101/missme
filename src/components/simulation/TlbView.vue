<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useVirtualizedWindow } from "../../lib/virtualizedWindow";
import { touchAndPrune } from "../../lib/lruCache";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

interface TlbEntry {
  valid: boolean;
  vpn: number;
  ppn: number;
  orderValue: number | null;
}
interface TlbSet {
  setIndex: number;
  entries: TlbEntry[];
}
interface TlbInfo {
  associativity: number;
  numSets: number;
}

const info = ref<TlbInfo | null>(null);
const hierarchyStore = useHierarchyConfigStore();

// The TLB label is only ever FIFO or LRU, and the order column disappears entirely for Random.
const orderColumnLabel = computed(() => (hierarchyStore.tlb.replacement === "FIFO" ? "FIFO" : "LRU"));
const showOrderColumn = computed(() => hierarchyStore.tlb.replacement !== "Random");
const gridColumnsStyle = computed(() => {
  const orderTrack = showOrderColumn.value ? "2.5rem " : "";
  return `4rem 2.5rem ${orderTrack}calc(10ch + 0.6rem) calc(10ch + 0.6rem)`;
});
const MAX_CACHED_SETS = 4000;
const ROW_HEIGHT = 26;
const WINDOW_SETS = 20_000;
const RECENTER_MARGIN_SETS = 500;

const setBlockHeight = computed(() => Math.max(1, info.value?.associativity ?? 1) * ROW_HEIGHT);
const sets = reactive(new Map<number, TlbSet>());

async function fetchPage(startSet: number, count: number) {
  try {
    const result = await invoke<TlbSet[]>("get_tlb_sets", { startSet, count });
    const touched: number[] = [];
    result.forEach((set) => {
      sets.set(set.setIndex, set);
      touched.push(set.setIndex);
    });
    touchAndPrune(sets, touched, MAX_CACHED_SETS);
  } catch (error) {
    console.error("Failed to fetch TLB sets:", error);
  }
}

const win = useVirtualizedWindow({
  // The block height depends on associativity, which is not known until the TLB info arrives.
  rowHeight: () => setBlockHeight.value,
  windowRows: WINDOW_SETS,
  recenterMarginRows: RECENTER_MARGIN_SETS,
  totalRows: () => info.value?.numSets ?? WINDOW_SETS,
  fetchPage,
});

async function loadInfo() {
  try {
    info.value = await invoke<TlbInfo>("get_tlb_info", {});
    sets.clear();
    win.resetWindow();
    // Waits for the page to render before measuring the scrolling area, or it always reads zero.
    await nextTick();
    win.measureViewport();
    await fetchPage(0, Math.max(0, win.range.value.end - win.range.value.start));
  } catch (error) {
    console.error("Failed to fetch TLB info:", error);
  }
}

function hexDigitsForBits(bits: number): number {
  return Math.max(1, Math.ceil(bits / 4));
}
const addressBits = computed(() => (hierarchyStore.isa === "RV32I" ? 32 : 64));
const vpnDigits = computed(() => {
  const pageSize = Math.max(1, hierarchyStore.pageSize);
  return hexDigitsForBits(addressBits.value - Math.log2(pageSize));
});
const ppnDigits = computed(() => {
  const numFrames = Math.max(1, hierarchyStore.ram.frames);
  return hexDigitsForBits(Math.log2(numFrames));
});
function hexVpn(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(vpnDigits.value, "0")}`;
}
function hexPpn(value: number): string {
  return `0x${(value >>> 0).toString(16).toUpperCase().padStart(ppnDigits.value, "0")}`;
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  await loadInfo();
  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "tlb_access") return;
    fetchPage(win.baseIndex.value + win.range.value.start, Math.max(0, win.range.value.end - win.range.value.start));
  });
  if (disposed) stopListening();
  else unlisten = stopListening;
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});

// A backup for when the scrolling area shows up later than expected, so it still gets measured and filled.
watch(
  () => win.containerRef.value,
  () => {
    win.measureViewport();
    fetchPage(win.baseIndex.value + win.range.value.start, Math.max(0, win.range.value.end - win.range.value.start));
  },
);
</script>

<template>
  <div class="simulation-tlb-view">
    <template v-if="info">
      <div class="simulation-tlb-outer">
        <div class="simulation-tlb-grid-wrapper">
          <div class="simulation-tlb-grid-header" :style="{ gridTemplateColumns: gridColumnsStyle }">
            <span class="simulation-tlb-cell simulation-tlb-cell-header">Set</span>
            <span class="simulation-tlb-cell simulation-tlb-cell-header">V</span>
            <span v-if="showOrderColumn" class="simulation-tlb-cell simulation-tlb-cell-header">{{ orderColumnLabel }}</span>
            <span class="simulation-tlb-cell simulation-tlb-cell-header">VPN</span>
            <span class="simulation-tlb-cell simulation-tlb-cell-header">PPN</span>
          </div>
          <div class="simulation-tlb-body" :ref="(el) => (win.containerRef.value = el as HTMLDivElement | null)" @scroll="win.onScroll">
            <div class="simulation-tlb-spacer" :style="{ height: `${win.range.value.totalHeight}px`, position: 'relative' }">
              <div
                v-for="(setIndex, i) in win.visibleIndices.value"
                :key="setIndex"
                class="simulation-tlb-set-block"
                :style="{ position: 'absolute', top: `${(win.range.value.start + i) * setBlockHeight}px`, height: `${setBlockHeight}px`, gridTemplateColumns: gridColumnsStyle }"
              >
                <template v-if="sets.get(setIndex)">
                  <span class="simulation-tlb-cell simulation-tlb-set-index" :style="{ gridRow: `1 / span ${sets.get(setIndex)!.entries.length}` }">{{ setIndex }}</span>
                  <template v-for="(entry, wi) in sets.get(setIndex)!.entries" :key="wi">
                    <template v-if="entry.valid">
                      <span class="simulation-tlb-cell">1</span>
                      <span v-if="showOrderColumn" class="simulation-tlb-cell">{{ entry.orderValue }}</span>
                      <span class="simulation-tlb-cell">{{ hexVpn(entry.vpn) }}</span>
                      <span class="simulation-tlb-cell">{{ hexPpn(entry.ppn) }}</span>
                    </template>
                    <template v-else>
                      <span v-for="n in (showOrderColumn ? 4 : 3)" :key="n" class="simulation-tlb-cell simulation-tlb-cell-empty">-</span>
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
.simulation-tlb-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
  color: var(--text);
  transition: color var(--theme-transition-duration) ease;
}
.simulation-tlb-outer {
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
.simulation-tlb-grid-wrapper {
  display: flex;
  flex-direction: column;
  width: fit-content;
  height: 100%;
  margin: 0 auto;
  transition: none;
}
.simulation-tlb-grid-header {
  display: grid;
  flex: 0 0 auto;
  background: var(--background);
  box-shadow: 0 2px 4px -2px rgba(0, 0, 0, 0.4);
  width: fit-content;
  transition: background-color var(--theme-transition-duration) ease;
}
.simulation-tlb-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  transition: none;
}
.simulation-tlb-set-block {
  display: grid;
  grid-auto-rows: 26px;
  width: fit-content;
  transition: none;
}
.simulation-tlb-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.6rem;
  box-sizing: border-box;
  transition: border-color var(--theme-transition-duration) ease;
}
.simulation-tlb-cell-header {
  font-weight: 600;
  opacity: 0.75;
}
.simulation-tlb-set-index {
  font-weight: 600;
  grid-column: 1;
}
.simulation-tlb-cell-empty {
  color: var(--text-muted, var(--text));
  opacity: 0.4;
  transition: border-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}
</style>
