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

const RAM_BLOCK_SIZE = 64;
const ROW_HEIGHT = 28;
const MAX_CACHED_BLOCKS = 4000;

const totalBlocks = computed(() => Math.max(0, Math.ceil(store.ram.size / RAM_BLOCK_SIZE)));

interface RamBlock {
  address: number;
  words: number[];
  populated: boolean;
}

const blocks = reactive(new Map<number, RamBlock>());
const containerRef = ref<HTMLDivElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);

const range = computed(() => computeVirtualRange(scrollTop.value, viewportHeight.value, ROW_HEIGHT, totalBlocks.value));

async function fetchVisibleWindow() {
  const start = range.value.start;
  const count = Math.max(0, range.value.end - start);
  if (count === 0) return;
  const startAddress = compileConfigStore.dataBase + start * RAM_BLOCK_SIZE;
  try {
    const result = await invoke<RamBlock[]>("get_ram_blocks", { startAddress, count });
    const touchedIndices: number[] = [];
    result.forEach((block, i) => {
      const index = start + i;
      blocks.set(index, block);
      touchedIndices.push(index);
    });
    touchAndPrune(blocks, touchedIndices, MAX_CACHED_BLOCKS);
  } catch (error) {
    console.error("Failed to fetch RAM blocks:", error);
  }
}

function onScroll(event: Event) {
  scrollTop.value = (event.target as HTMLDivElement).scrollTop;
  fetchVisibleWindow();
}

function hex(value: number): string {
  return (value >>> 0).toString(16);
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  if (containerRef.value) viewportHeight.value = containerRef.value.clientHeight;
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
});
</script>

<template>
  <div class="simulation-ram-view" ref="containerRef" @scroll="onScroll">
    <div class="simulation-ram-spacer" :style="{ height: `${range.totalHeight}px`, position: 'relative' }">
      <div
        v-for="index in Array.from({ length: Math.max(0, range.end - range.start) }, (_, i) => range.start + i)"
        :key="index"
        class="simulation-ram-row"
        :style="{ position: 'absolute', top: `${index * ROW_HEIGHT}px`, height: `${ROW_HEIGHT}px` }"
      >
        <template v-if="blocks.get(index)">
          <span class="simulation-ram-address">0x{{ hex(blocks.get(index)!.address) }}</span>
          <span
            v-for="(word, wi) in blocks.get(index)!.words"
            :key="wi"
            class="simulation-ram-word"
            :class="{ 'simulation-ram-word-empty': !blocks.get(index)!.populated }"
          >{{ hex(word) }}</span>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.simulation-ram-view {
  padding: 0.5rem;
  font-size: 0.75rem;
  font-family: monospace;
  overflow-y: auto;
  height: 100%;
  box-sizing: border-box;
}

.simulation-ram-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
}

.simulation-ram-address {
  color: var(--text-muted, var(--text));
  opacity: 0.7;
  min-width: 5rem;
}

.simulation-ram-word-empty {
  opacity: 0.3;
}
</style>
