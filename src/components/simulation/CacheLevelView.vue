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
      <div class="simulation-cache-spacer" :style="{ height: `${range.totalHeight}px`, position: 'relative' }">
        <div
          v-for="set in visibleSets"
          :key="set.setIndex"
          class="simulation-cache-set-row"
          :style="{ position: 'absolute', top: `${set.setIndex * ROW_HEIGHT}px`, height: `${ROW_HEIGHT}px` }"
        >
          <span class="simulation-cache-set-index">{{ set.setIndex }}</span>
          <div v-for="(way, wi) in set.ways" :key="wi" class="simulation-cache-way" :class="{ 'simulation-cache-way-empty': !way.valid }">
            <template v-if="way.valid">
              <span class="simulation-cache-way-dirty">{{ way.dirty ? "D" : "" }}</span>
              <span class="simulation-cache-way-lru">{{ way.recencyRank }}</span>
              <span class="simulation-cache-way-tag">{{ hex(way.tag) }}</span>
              <span v-for="(word, wj) in way.words" :key="wj" class="simulation-cache-word">{{ hex(word) }}</span>
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

.simulation-cache-set-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  width: 100%;
}

.simulation-cache-way {
  display: flex;
  gap: 0.3rem;
  padding: 0.2rem 0.4rem;
  border: 1px solid var(--secondary);
  border-radius: 4px;
  min-width: 6rem;
}

.simulation-cache-way-empty {
  opacity: 0.3;
}
</style>
