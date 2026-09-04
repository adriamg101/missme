<script setup lang="ts">
import { computed } from "vue";
import HexField from "../ui/HexField.vue";
import { useCompileConfigStore } from "../../stores/compileConfig";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const store = useCompileConfigStore();
const hierarchyStore = useHierarchyConfigStore();

const U32_MAX = 0xffffffff;

// This is a rough guess for the size of the program before it has been compiled, only used to warn about data sitting too close to code.
const MIN_TEXT_GAP = 0x100000;

function toHex8(value: number): string {
  return value.toString(16).toUpperCase().padStart(8, "0");
}

function fromHex(value: string): number {
  return Math.min(parseInt(value, 16), U32_MAX);
}

const textBaseHex = computed(() => toHex8(store.textBase));

const dataBaseHex = computed({
  get: () => toHex8(store.dataBase),
  set: (value: string) => {
    // An empty field means the user is still typing, so we keep the last real value instead of treating it as zero.
    if (value === "") return;
    store.setDataBase(fromHex(value));
  },
});

// These are only warnings, they never stop the user from typing an invalid value.
const textGap = computed(() => store.lastCompiledTextSize ?? MIN_TEXT_GAP);
const overlapsText = computed(() => store.dataBase < store.textBase + textGap.value);

// RAM's real capacity comes from frames and page size when virtual memory is on, and from the plain size otherwise.
const ramCapacity = computed(() =>
  hierarchyStore.vmEnabled ? hierarchyStore.ram.frames * hierarchyStore.pageSize : hierarchyStore.ram.size,
);

// This can only be checked once the program has actually been compiled, there is no way to guess it beforehand.
const exceedsRam = computed(
  () => store.lastCompiledDataSize !== null && store.lastCompiledDataSize > ramCapacity.value,
);
</script>

<template>
  <div class="riscv-compile-config-form">
    <span class="riscv-compile-config-form-title">Memory map</span>
    <div class="riscv-compile-config-form-row">
      <HexField label=".text" aria-label=".text base address" :model-value="textBaseHex" readonly />
      <span class="riscv-compile-config-form-divider">|</span>
      <HexField label=".data" aria-label=".data base address" v-model="dataBaseHex" />
    </div>
    <span v-if="overlapsText" class="riscv-compile-config-form-warning">
      .data is too close to .text{{ store.lastCompiledTextSize !== null ? "" : " — they may overlap once the program is compiled" }}.
    </span>
    <span v-if="exceedsRam" class="riscv-compile-config-form-warning">
      The compiled .data segment doesn't fit in the configured RAM size.
    </span>
  </div>
</template>

<style scoped>
.riscv-compile-config-form {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.riscv-compile-config-form-title {
  font-size: 0.75rem;
  color: var(--secondary);
}

.riscv-compile-config-form-row {
  display: flex;
  align-items: flex-end;
  gap: 0.6rem;
}

.riscv-compile-config-form-divider {
  padding-bottom: 0.3rem;
  color: var(--secondary);
}

.riscv-compile-config-form-warning {
  font-size: 0.7rem;
  color: var(--diagram-miss-color);
}
</style>
