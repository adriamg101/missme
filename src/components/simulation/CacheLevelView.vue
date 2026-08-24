<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { computeVirtualRange } from "../../lib/virtualScroll";

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
interface CacheLevelSnapshot {
  associativity: number;
  blockSize: number;
  numSets: number;
  wordsPerLine: number;
  sets: CacheSet[];
}

const snapshot = ref<CacheLevelSnapshot | null>(null);

const ROW_HEIGHT = 44;
const containerRef = ref<HTMLDivElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);

const range = computed(() => computeVirtualRange(scrollTop.value, viewportHeight.value, ROW_HEIGHT, snapshot.value?.sets.length ?? 0));
const visibleSets = computed(() => (snapshot.value ? snapshot.value.sets.slice(range.value.start, range.value.end) : []));

function onScroll(event: Event) {
  scrollTop.value = (event.target as HTMLDivElement).scrollTop;
}

function hex(value: number): string {
  return (value >>> 0).toString(16);
}

const wayColumnCount = computed(() => 3 + (snapshot.value?.wordsPerLine ?? 0));
const rowGridStyle = computed(() => ({
  display: "grid" as const,
  gridTemplateColumns: `4rem repeat(${snapshot.value?.associativity ?? 0}, auto)`,
}));
const wayGridStyle = computed(() => ({
  display: "grid" as const,
  gridTemplateColumns: `repeat(${wayColumnCount.value}, minmax(2.75rem, auto))`,
}));

async function loadLevel() {
  if (levelIndex.value < 0) {
    snapshot.value = null;
    return;
  }
  try {
    snapshot.value = await invoke<CacheLevelSnapshot>("get_cache_level", { levelIndex: levelIndex.value, side: props.side });
  } catch (error) {
    console.error("Failed to fetch cache level:", error);
  }
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  if (containerRef.value) viewportHeight.value = containerRef.value.clientHeight;
  await loadLevel();

  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "cache_access") return;
    if (event.payload.levelIndex !== levelIndex.value) return;
    loadLevel();
  });
  if (disposed) stopListening();
  else unlisten = stopListening;
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});

watch(() => [props.levelId, props.side], loadLevel);
</script>

<template>
  <div class="simulation-cache-level-view" ref="containerRef" @scroll="onScroll">
    <template v-if="snapshot">
      <div class="simulation-cache-header">Associativitat: {{ snapshot.associativity }} | Mida de bloc: {{ snapshot.blockSize }}</div>
      <div class="simulation-cache-grid-header" :style="rowGridStyle">
        <span class="simulation-cache-cell simulation-cache-cell-header">Set</span>
        <div v-for="w in snapshot.associativity" :key="w" class="simulation-cache-way" :style="wayGridStyle">
          <span class="simulation-cache-cell simulation-cache-cell-header">D</span>
          <span class="simulation-cache-cell simulation-cache-cell-header">LRU</span>
          <span class="simulation-cache-cell simulation-cache-cell-header">Tag</span>
          <span v-for="wi in snapshot.wordsPerLine" :key="wi" class="simulation-cache-cell simulation-cache-cell-header">W{{ wi - 1 }}</span>
        </div>
      </div>
      <div class="simulation-cache-spacer" :style="{ height: `${range.totalHeight}px`, position: 'relative' }">
        <div
          v-for="set in visibleSets"
          :key="set.setIndex"
          class="simulation-cache-set-row"
          :style="{ position: 'absolute', top: `${set.setIndex * ROW_HEIGHT}px`, height: `${ROW_HEIGHT}px`, ...rowGridStyle }"
        >
          <span class="simulation-cache-cell simulation-cache-set-index">{{ set.setIndex }}</span>
          <div v-for="(way, wi) in set.ways" :key="wi" class="simulation-cache-way" :class="{ 'simulation-cache-way-empty': !way.valid }" :style="wayGridStyle">
            <template v-if="way.valid">
              <span class="simulation-cache-cell">{{ way.dirty ? "D" : "-" }}</span>
              <span class="simulation-cache-cell">{{ way.recencyRank }}</span>
              <span class="simulation-cache-cell">{{ hex(way.tag) }}</span>
              <span v-for="(word, wj) in way.words" :key="wj" class="simulation-cache-cell">{{ hex(word) }}</span>
            </template>
            <template v-else>
              <span v-for="n in wayColumnCount" :key="n" class="simulation-cache-cell"></span>
            </template>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.simulation-cache-level-view {
  padding: 0.5rem;
  font-size: 0.75rem;
  font-family: monospace;
  overflow-y: auto;
  height: 100%;
  box-sizing: border-box;
}

.simulation-cache-header {
  font-family: sans-serif;
  font-size: 0.8rem;
  margin-bottom: 0.5rem;
}

.simulation-cache-grid-header {
  align-items: stretch;
  position: sticky;
  top: 0;
  background: var(--background);
  z-index: 1;
}

.simulation-cache-set-row {
  align-items: stretch;
  width: 100%;
}

.simulation-cache-way {
  align-items: stretch;
}

.simulation-cache-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.35rem;
  box-sizing: border-box;
}

.simulation-cache-cell-header {
  font-weight: 600;
  opacity: 0.75;
}

.simulation-cache-way-empty .simulation-cache-cell {
  opacity: 0.25;
}
</style>
