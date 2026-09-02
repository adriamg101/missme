<script setup lang="ts">
import { computed } from "vue";
import HexField from "../ui/HexField.vue";
import { useCompileConfigStore } from "../../stores/compileConfig";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const store = useCompileConfigStore();
const hierarchyStore = useHierarchyConfigStore();

const U32_MAX = 0xffffffff;

// Before the program is compiled for the first time, we don't actually know
// how big .text will be, since the user might edit dataBase before ever
// compiling. So this is just a rough guess, not an exact check. 1 MiB is a
// generous size for any program this simulator would realistically run,
// used only to catch the common mistake of placing .data right at or just
// after .text with no real gap. Once a compile has happened,
// `lastCompiledTextSize` gives the real size and this guess is no longer used.
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
    // An empty field just means the user is in the middle of typing, not
    // that they picked base address 0, so keep the previous valid dataBase
    // in the store instead of quietly turning it into 0. The value shown
    // above stays as the last valid one until the field has something in it
    // again; HexField leaves the input itself empty in the meantime.
    if (value === "") return;
    store.setDataBase(fromHex(value));
  },
});

// These are just warnings, not blockers. The app never stops the user from
// typing an invalid value, it just points out the problem (same approach
// used for the dirtyPercent check in CacheConfigForm.vue).
const textGap = computed(() => store.lastCompiledTextSize ?? MIN_TEXT_GAP);
const overlapsText = computed(() => store.dataBase < store.textBase + textGap.value);

// This can only be checked once a compile has actually produced a .data
// size. Unlike the check above, there's no fallback guess here, since there's
// no reasonable way to estimate how big the program's data will be.
const exceedsRam = computed(
  () => store.lastCompiledDataSize !== null && store.lastCompiledDataSize > hierarchyStore.ram.size,
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
