<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive } from "vue";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const props = defineProps<{ levelId: string }>();
const store = useHierarchyConfigStore();

const levelIndex = computed(() => store.hierarchy.findIndex((l) => l.id === props.levelId));
const level = computed(() => store.hierarchy[levelIndex.value]);
// Harvard i/d sides share the same associativity/block size in this
// project's model, so either side's config is representative for display.
const config = computed(() => level.value?.unified?.config ?? level.value?.i?.config ?? level.value?.d?.config);

const counts = reactive({ hits: 0, misses: 0 });

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "cache_access") return;
    if (event.payload.levelIndex !== levelIndex.value) return;
    if (event.payload.outcome === "hit") counts.hits++;
    else if (event.payload.outcome === "miss") counts.misses++;
  });
  if (disposed) {
    stopListening();
  } else {
    unlisten = stopListening;
  }
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<template>
  <div class="simulation-cache-level-view" v-if="level && config">
    <p>Associativitat: {{ config.associativity }}</p>
    <p>Mida de bloc: {{ config.blockSize }}</p>
    <p>Encerts: <span class="simulation-cache-hits">{{ counts.hits }}</span></p>
    <p>Falles: <span class="simulation-cache-misses">{{ counts.misses }}</span></p>
  </div>
</template>

<style scoped>
.simulation-cache-level-view {
  padding: 0.75rem;
  font-size: 0.8rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
</style>
