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
    <div class="simulation-registers-grid">
      <template v-for="(value, i) in intRegs" :key="`x${i}`">
        <span class="simulation-register-cell simulation-register-name">x{{ i }}</span>
        <span class="simulation-register-cell">{{ value }}</span>
      </template>
    </div>
    <div class="simulation-registers-grid">
      <template v-for="(value, i) in fpRegs" :key="`f${i}`">
        <span class="simulation-register-cell simulation-register-name">f{{ i }}</span>
        <span class="simulation-register-cell">{{ value }}</span>
      </template>
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

.simulation-registers-grid {
  display: grid;
  grid-template-columns: 2.5rem 1fr;
  align-items: stretch;
  font-size: 0.75rem;
  font-family: monospace;
  flex: 1 1 0;
}

.simulation-register-cell {
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--secondary);
  padding: 0.15rem 0.35rem;
  box-sizing: border-box;
}

.simulation-register-name {
  color: var(--text-muted, var(--text));
  opacity: 0.7;
  justify-content: flex-start;
}
</style>
