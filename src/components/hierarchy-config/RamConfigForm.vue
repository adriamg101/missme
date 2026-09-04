<!-- src/components/hierarchy-config/RamConfigForm.vue -->
<script setup lang="ts">
import { computed } from "vue";
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";
import NumberStepper from "../ui/NumberStepper.vue";
import SelectField from "../ui/SelectField.vue";
import ConfigGrid from "./ConfigGrid.vue";
import type { Hierarchy, ISA, RamConfig } from "../../lib/hierarchy/model";
import { defaultRamConfig, maxPositions } from "../../lib/hierarchy/model";
import { maxCacheByteCapacity } from "../../lib/hierarchy/derive";

const props = defineProps<{ config: RamConfig; hierarchy: Hierarchy; vmEnabled: boolean; isa: ISA; pageSize: number }>();
const emit = defineEmits<{ update: [patch: Partial<RamConfig>] }>();

const frameAllocationOptions = ["sequential", "random"].map((value) => ({ value, label: value }));

// RAM must always hold more than the biggest cache configured, so frames cannot go below this.
const minFrames = computed(() => Math.max(1, (maxCacheByteCapacity(props.hierarchy) * 2) / props.pageSize));

// RAM can never be bigger than the address space the chosen ISA can reach.
const maxFrames = computed(() => 2 ** maxPositions(props.isa) / props.pageSize);

// A page table level can never claim more bits than the address has left after the page offset.
const maxLevels = computed(() => Math.max(1, maxPositions(props.isa) - Math.log2(props.pageSize)));

// Falls back to the default when an older saved config is missing these fields.
const safeFrames = computed(() => (Number.isFinite(props.config.frames) ? props.config.frames : defaultRamConfig().frames));
const safeLatency = computed(() =>
  Number.isFinite(props.config.latency) ? props.config.latency : defaultRamConfig().latency,
);
const safeDirtyPercent = computed(() =>
  Number.isFinite(props.config.dirtyPercent) ? props.config.dirtyPercent : defaultRamConfig().dirtyPercent,
);
const safePageTableLevels = computed(() =>
  Number.isFinite(props.config.pageTableLevels) ? props.config.pageTableLevels : defaultRamConfig().pageTableLevels,
);

function onDirtyPercentChange(value: number) {
  emit("update", { dirtyPercent: Math.min(100, Math.max(0, value)) });
}
</script>

<template>
  <form class="ram-config-form" @submit.prevent>
    <ConfigGrid>
      <PowerOfTwoStepper
        v-if="vmEnabled"
        label="Frames"
        :model-value="safeFrames"
        :min="minFrames"
        :max="maxFrames"
        aria-label="RAM frame count"
        @update:model-value="emit('update', { frames: $event })"
      />

      <NumberStepper
        v-if="vmEnabled"
        label="Dirty pages at start (%)"
        :model-value="safeDirtyPercent"
        :min="0"
        :max="100"
        :step="5"
        aria-label="RAM dirty pages at start"
        @update:model-value="onDirtyPercentChange"
      />

      <NumberStepper
        v-if="vmEnabled"
        label="Page table levels"
        :model-value="safePageTableLevels"
        :min="1"
        :max="maxLevels"
        aria-label="Page table levels"
        @update:model-value="(v) => emit('update', { pageTableLevels: Math.max(1, Number.isFinite(v) ? v : 1) })"
      />

      <SelectField
        v-if="vmEnabled"
        label="Frame allocation"
        :model-value="config.frameAllocation"
        :options="frameAllocationOptions"
        aria-label="RAM frame allocation"
        @update:model-value="emit('update', { frameAllocation: $event as RamConfig['frameAllocation'] })"
      />

      <NumberStepper
        label="Latency (cycles)"
        :model-value="safeLatency"
        :min="1"
        aria-label="RAM latency"
        @update:model-value="emit('update', { latency: $event })"
      />
    </ConfigGrid>
  </form>
</template>
