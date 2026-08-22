<!-- src/components/hierarchy-config/TlbConfigForm.vue -->
<script setup lang="ts">
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";
import type { TlbConfig, TlbReplacementPolicy } from "../../lib/hierarchy/model";
import type { PageWalkTarget } from "../../lib/hierarchy/derive";

defineProps<{ tlb: TlbConfig; pageWalkTargets: PageWalkTarget[] }>();
const emit = defineEmits<{ update: [patch: Partial<TlbConfig>] }>();

function onPageTableLevelsChange(event: Event) {
  const raw = Number((event.target as HTMLInputElement).value);
  emit("update", { pageTableLevels: Math.max(1, Number.isFinite(raw) ? raw : 1) });
}
</script>

<template>
  <form class="tlb-config-form">
    <label>
      Page size
      <PowerOfTwoStepper :model-value="tlb.pageSize" @update:model-value="(v) => emit('update', { pageSize: v })" />
    </label>

    <label>
      Page table levels
      <input
        type="number"
        aria-label="Page table levels"
        :value="tlb.pageTableLevels"
        min="1"
        @change="onPageTableLevelsChange"
      />
    </label>

    <label>
      Page walk start
      <select
        aria-label="Page walk start"
        :value="tlb.pageWalkStart"
        @change="emit('update', { pageWalkStart: ($event.target as HTMLSelectElement).value })"
      >
        <option v-for="target in pageWalkTargets" :key="target.id" :value="target.id">{{ target.label }}</option>
      </select>
    </label>

    <label>
      Entries
      <input
        type="number"
        aria-label="Entries"
        :value="tlb.entries"
        min="1"
        @change="emit('update', { entries: Number(($event.target as HTMLInputElement).value) })"
      />
    </label>

    <label>
      Associativity
      <input
        type="number"
        aria-label="TLB associativity"
        :value="tlb.associativity"
        min="1"
        :max="tlb.entries"
        @change="emit('update', { associativity: Number(($event.target as HTMLInputElement).value) })"
      />
    </label>

    <label>
      Replacement policy
      <select
        aria-label="TLB replacement policy"
        :value="tlb.replacement"
        @change="emit('update', { replacement: ($event.target as HTMLSelectElement).value as TlbReplacementPolicy })"
      >
        <option>LRU</option>
        <option>FIFO</option>
        <option>Random</option>
        <option>PLRU</option>
      </select>
    </label>
  </form>
</template>
