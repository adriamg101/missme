<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSimulationStore } from "../../stores/simulation";

const INT_ABI_NAMES = [
  "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2",
  "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5",
  "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7",
  "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

const FP_ABI_NAMES = [
  "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7",
  "fs0", "fs1", "fa0", "fa1", "fa2", "fa3", "fa4", "fa5",
  "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7",
  "fs8", "fs9", "fs10", "fs11", "ft8", "ft9", "ft10", "ft11",
];

const simulationStore = useSimulationStore();
const hasFloatExtension = computed(
  () => simulationStore.activeExtensions.includes("F") || simulationStore.activeExtensions.includes("D"),
);

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
        <span class="simulation-register-cell simulation-register-alias">{{ INT_ABI_NAMES[i] }}</span>
        <span class="simulation-register-cell">{{ value }}</span>
      </template>
    </div>
    <div v-if="hasFloatExtension" class="simulation-registers-grid">
      <template v-for="(value, i) in fpRegs" :key="`f${i}`">
        <span class="simulation-register-cell simulation-register-name">f{{ i }}</span>
        <span class="simulation-register-cell simulation-register-alias">{{ FP_ABI_NAMES[i] }}</span>
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
  grid-template-columns: 2.5rem 3.5rem 1fr;
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
