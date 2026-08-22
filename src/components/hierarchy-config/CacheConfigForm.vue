<!-- src/components/hierarchy-config/CacheConfigForm.vue -->
<script setup lang="ts">
import { computed } from "vue";
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";
import type { CacheConfig, ReplacementPolicy, WritePolicy } from "../../lib/hierarchy/model";

const props = defineProps<{ config: CacheConfig; cacheSizeExp: number; isInstructionCache: boolean }>();
const emit = defineEmits<{ update: [patch: Partial<CacheConfig>] }>();

const maxAssociativity = computed(() => Math.max(1, Math.floor(2 ** props.cacheSizeExp / props.config.blockSize)));

function onBlockSizeChange(value: number) {
  const patch: Partial<CacheConfig> = { blockSize: value };
  const newMax = Math.max(1, Math.floor(2 ** props.cacheSizeExp / value));
  if (props.config.associativity > newMax) patch.associativity = newMax;
  emit("update", patch);
}

function onAssociativityChange(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  emit("update", { associativity: Math.min(Math.max(1, value), maxAssociativity.value) });
}
</script>

<template>
  <form class="cache-config-form">
    <label>
      Block size
      <PowerOfTwoStepper :model-value="config.blockSize" @update:model-value="onBlockSizeChange" />
    </label>

    <label>
      Associativity
      <input
        type="number"
        aria-label="Associativity"
        :value="config.associativity"
        min="1"
        :max="maxAssociativity"
        @change="onAssociativityChange"
      />
    </label>

    <label>
      Replacement policy
      <select
        aria-label="Replacement policy"
        :value="config.replacement"
        @change="emit('update', { replacement: ($event.target as HTMLSelectElement).value as ReplacementPolicy })"
      >
        <option>LRU</option>
        <option>FIFO</option>
        <option>Random</option>
        <option>LFU</option>
        <option>PLRU</option>
      </select>
    </label>

    <template v-if="!isInstructionCache">
      <label>
        Write policy
        <select
          aria-label="Write policy"
          :value="config.writePolicy"
          @change="emit('update', { writePolicy: ($event.target as HTMLSelectElement).value as WritePolicy })"
        >
          <option value="write-through">write-through</option>
          <option value="write-back">write-back</option>
        </select>
      </label>

      <label>
        Write allocate
        <input
          type="checkbox"
          aria-label="Write allocate"
          :checked="config.writeAllocate"
          @change="emit('update', { writeAllocate: ($event.target as HTMLInputElement).checked })"
        />
      </label>
    </template>

    <label>
      Hit time (cycles)
      <input
        type="number"
        aria-label="Hit time"
        :value="config.hitTime"
        min="1"
        @change="emit('update', { hitTime: Number(($event.target as HTMLInputElement).value) })"
      />
    </label>
  </form>
</template>
