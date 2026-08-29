<!-- src/components/hierarchy-config/CacheConfigForm.vue -->
<script setup lang="ts">
import { computed } from "vue";
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";
import NumberStepper from "../ui/NumberStepper.vue";
import SelectField from "../ui/SelectField.vue";
import ConfigGrid from "./ConfigGrid.vue";
import type { CacheConfig, ISA, ReplacementPolicy, WritePolicy } from "../../lib/hierarchy/model";
import { defaultCacheConfig } from "../../lib/hierarchy/model";

const props = defineProps<{ config: CacheConfig; cacheSizeExp: number; isInstructionCache: boolean; isa: ISA }>();
const emit = defineEmits<{ update: [patch: Partial<CacheConfig>]; "apply-to-all": [] }>();

// cacheSizeExp is the slider's position — the number of entries (tags) in
// this cache, not a byte count — so associativity (ways) can't exceed that
// entry count regardless of block size (sets = entries / associativity >= 1).
const maxAssociativity = computed(() => 2 ** props.cacheSizeExp);

// A block must be able to hold at least one word — 4 bytes on RV32I, 8 on RV64I.
const minBlockSize = computed(() => (props.isa === "RV32I" ? 4 : 8));

// Defends against a config persisted before CacheConfig gained `dirtyPercent`
// (e.g. localStorage saved by an older version of this app) — without this,
// an undefined value here turns every +/- click into a NaN that never visibly
// changes (NaN === NaN is false, so the stepper never sees "no change").
const safeDirtyPercent = computed(() =>
  Number.isFinite(props.config.dirtyPercent) ? props.config.dirtyPercent : defaultCacheConfig().dirtyPercent,
);

const replacementOptions = ["LRU", "FIFO", "Random", "LFU"].map((value) => ({ value, label: value }));
const writePolicyOptions = [
  { value: "write-through", label: "write-through" },
  { value: "write-back", label: "write-back" },
];

function onBlockSizeChange(value: number) {
  emit("update", { blockSize: value });
}

function onDirtyPercentChange(value: number) {
  emit("update", { dirtyPercent: Math.min(100, Math.max(0, value)) });
}
</script>

<template>
  <form class="cache-config-form" @submit.prevent>
    <div class="cache-config-form-apply-all">
      <button type="button" @click="emit('apply-to-all')">Apply all</button>
    </div>

    <ConfigGrid>
      <PowerOfTwoStepper
        label="Block size"
        aria-label="Block size"
        :model-value="config.blockSize"
        :min="minBlockSize"
        @update:model-value="onBlockSizeChange"
      />

      <PowerOfTwoStepper
        label="Associativity"
        :model-value="config.associativity"
        :min="1"
        :max="maxAssociativity"
        aria-label="Associativity"
        @update:model-value="emit('update', { associativity: $event })"
      />

      <SelectField
        label="Replacement policy"
        :model-value="config.replacement"
        :options="replacementOptions"
        aria-label="Replacement policy"
        @update:model-value="emit('update', { replacement: $event as ReplacementPolicy })"
      />

      <template v-if="!isInstructionCache">
        <SelectField
          label="Write policy"
          :model-value="config.writePolicy"
          :options="writePolicyOptions"
          aria-label="Write policy"
          @update:model-value="emit('update', { writePolicy: $event as WritePolicy })"
        />

        <div class="cache-config-form-checkbox">
          <span class="cache-config-form-checkbox-label">Write allocate</span>
          <input
            type="checkbox"
            aria-label="Write allocate"
            :checked="config.writeAllocate"
            @change="emit('update', { writeAllocate: ($event.target as HTMLInputElement).checked })"
          />
        </div>

        <NumberStepper
          v-if="config.writePolicy === 'write-back'"
          label="Dirty at start (%)"
          :model-value="safeDirtyPercent"
          :min="0"
          :max="100"
          :step="5"
          aria-label="Dirty at start"
          @update:model-value="onDirtyPercentChange"
        />
      </template>

      <NumberStepper
        label="Hit time (cycles)"
        :model-value="config.hitTime"
        :min="1"
        aria-label="Hit time"
        @update:model-value="emit('update', { hitTime: $event })"
      />
    </ConfigGrid>
  </form>
</template>

<style scoped>
.cache-config-form-checkbox {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
  font-size: 0.875rem;
  color: var(--text);
}

.cache-config-form input[type="checkbox"] {
  width: 1.25rem;
  height: 1.25rem;
  accent-color: var(--accent);
}

.cache-config-form-apply-all {
  margin-bottom: 0.75rem;
}

.cache-config-form-apply-all button {
  padding: 0.4rem 0.75rem;
  border-radius: 6px;
  border: 1px solid var(--secondary);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 0.8rem;
  cursor: pointer;
}

.cache-config-form-apply-all button:hover {
  border-color: var(--accent);
}
</style>
