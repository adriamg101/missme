<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive } from "vue";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const store = useHierarchyConfigStore();
const counts = reactive({ accesses: 0 });

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    if (event.payload.kind !== "ram_access") return;
    counts.accesses++;
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
  <div class="simulation-ram-view">
    <p>Latencia: {{ store.ram.latency }}</p>
    <p>Accessos: <span class="simulation-ram-accesses">{{ counts.accesses }}</span></p>
  </div>
</template>

<style scoped>
.simulation-ram-view {
  padding: 0.75rem;
  font-size: 0.8rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
</style>
