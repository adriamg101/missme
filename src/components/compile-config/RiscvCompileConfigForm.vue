<script setup lang="ts">
import { computed } from "vue";
import HexField from "../ui/HexField.vue";
import { useCompileConfigStore } from "../../stores/compileConfig";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const store = useCompileConfigStore();
const hierarchyStore = useHierarchyConfigStore();

const U32_MAX = 0xffffffff;

// Before the first successful compile, the program's actual .text size
// isn't known yet (the user may edit dataBase before ever compiling) — so
// this is a best-effort heuristic, not an exact collision check. 1 MiB is a
// generous upper bound for any program this simulator is realistically
// used with, used purely to catch the common mistake of setting .data at
// or just after .text with no real gap. Once a compile has happened,
// `lastCompiledTextSize` gives the exact size and this fallback is unused.
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
    // An empty field means the user is mid-edit, not that they explicitly
    // chose base address 0 — keep the previous valid dataBase in the store
    // rather than silently coercing it to 0. The getter above then keeps
    // showing the last valid value once the field regains a value; the
    // input itself is left empty by HexField until then.
    if (value === "") return;
    store.setDataBase(fromHex(value));
  },
});

// Non-blocking warnings only — the app never prevents an invalid value from
// being typed, it just surfaces the problem (same house style as e.g. the
// dirtyPercent NaN guard in CacheConfigForm.vue).
const textGap = computed(() => store.lastCompiledTextSize ?? MIN_TEXT_GAP);
const overlapsText = computed(() => store.dataBase < store.textBase + textGap.value);

// Only checkable once a compile has actually produced a .data size — no
// fallback heuristic here, unlike the collision check above, since there's
// no sensible guess for "how big will the program's data be".
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
