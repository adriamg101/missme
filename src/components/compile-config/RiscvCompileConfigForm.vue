<script setup lang="ts">
import { computed } from "vue";
import HexField from "../ui/HexField.vue";
import { useCompileConfigStore } from "../../stores/compileConfig";

const store = useCompileConfigStore();

const U32_MAX = 0xffffffff;

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
    // An empty field means the user is mid-edit, not that they explicitly
    // chose base address 0 — keep the previous valid dataBase in the store
    // rather than silently coercing it to 0. The getter above then keeps
    // showing the last valid value once the field regains a value; the
    // input itself is left empty by HexField until then.
    if (value === "") return;
    store.setDataBase(fromHex(value));
  },
});
</script>

<template>
  <div class="riscv-compile-config-form">
    <span class="riscv-compile-config-form-title">Memory map</span>
    <div class="riscv-compile-config-form-row">
      <HexField label=".text" aria-label=".text base address" :model-value="textBaseHex" readonly />
      <span class="riscv-compile-config-form-divider">|</span>
      <HexField label=".data" aria-label=".data base address" v-model="dataBaseHex" />
    </div>
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
</style>
