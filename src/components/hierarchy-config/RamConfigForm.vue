<!-- src/components/hierarchy-config/RamConfigForm.vue -->
<script setup lang="ts">
import { computed } from "vue";
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";
import NumberStepper from "../ui/NumberStepper.vue";
import ConfigGrid from "./ConfigGrid.vue";
import type { Hierarchy, ISA, RamConfig } from "../../lib/hierarchy/model";
import { defaultRamConfig, maxPositions } from "../../lib/hierarchy/model";
import { maxCacheByteCapacity } from "../../lib/hierarchy/derive";

const props = defineProps<{ config: RamConfig; hierarchy: Hierarchy; vmEnabled: boolean; isa: ISA }>();
const emit = defineEmits<{ update: [patch: Partial<RamConfig>] }>();

// RAM must always be a power of two, and strictly bigger than the biggest
// cache level's actual byte capacity currently configured (for example, if
// L3 is the biggest, RAM is the next power of two above L3's real size in
// bytes, not its entry count). Doubling maxCacheByteCapacity, which is
// already a power of two, gives exactly that next power of two.
const minSize = computed(() => maxCacheByteCapacity(props.hierarchy) * 2);

// RAM can't be bigger than the address space the ISA can actually reach
// (2^32 for RV32I, 2^64 for RV64I).
const maxSize = computed(() => 2 ** maxPositions(props.isa));

// Protects against a saved config from before RamConfig had `size` in this
// shape (for example, data saved by an older version of the app). Without
// this, a missing value here would show up as "NaN" instead of a number.
const safeSize = computed(() => (Number.isFinite(props.config.size) ? props.config.size : defaultRamConfig().size));
const safeLatency = computed(() =>
  Number.isFinite(props.config.latency) ? props.config.latency : defaultRamConfig().latency,
);
const safeDirtyPercent = computed(() =>
  Number.isFinite(props.config.dirtyPercent) ? props.config.dirtyPercent : defaultRamConfig().dirtyPercent,
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
        label="Size"
        :model-value="safeSize"
        :min="minSize"
        :max="maxSize"
        aria-label="RAM size"
        @update:model-value="emit('update', { size: $event })"
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
        label="Latency (cycles)"
        :model-value="safeLatency"
        :min="1"
        aria-label="RAM latency"
        @update:model-value="emit('update', { latency: $event })"
      />
    </ConfigGrid>
  </form>
</template>
