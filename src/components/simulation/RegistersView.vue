<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const intRegs = reactive<number[]>(Array(32).fill(0));
const fpRegs = reactive<number[]>(Array(32).fill(0));

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  try {
    const snapshot = await invoke<{ int: number[]; fp: number[] }>("get_registers", {});
    if (snapshot?.int) {
      snapshot.int.forEach((value, i) => (intRegs[i] = value));
    }
    if (snapshot?.fp) {
      snapshot.fp.forEach((value, i) => (fpRegs[i] = value));
    }
  } catch (error) {
    console.error("Failed to fetch registers:", error);
  }

  try {
    const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
      const kind = event.payload.kind;
      if (kind === "reg_write") {
        intRegs[event.payload.register as number] = event.payload.value as number;
      } else if (kind === "fp_reg_write") {
        fpRegs[event.payload.register as number] = event.payload.value as number;
      }
    });
    if (disposed) {
      stopListening();
    } else {
      unlisten = stopListening;
    }
  } catch (error) {
    console.error("Failed to listen for events:", error);
  }
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<template>
  <div class="simulation-registers-view">
    <div class="simulation-registers-group">
      <div v-for="(value, i) in intRegs" :key="`x${i}`" class="simulation-register-row">
        <span class="simulation-register-name">x{{ i }}</span>
        <span class="simulation-register-value">{{ value }}</span>
      </div>
    </div>
    <div class="simulation-registers-group">
      <div v-for="(value, i) in fpRegs" :key="`f${i}`" class="simulation-register-row">
        <span class="simulation-register-name">f{{ i }}</span>
        <span class="simulation-register-value">{{ value }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.simulation-registers-view {
  display: flex;
  gap: 1rem;
  padding: 0.75rem;
  overflow-y: auto;
}

.simulation-registers-group {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  flex: 1 1 0;
}

.simulation-register-row {
  display: flex;
  justify-content: space-between;
  font-size: 0.75rem;
  font-family: monospace;
}

.simulation-register-name {
  color: var(--text-muted, var(--text));
  opacity: 0.7;
}
</style>
