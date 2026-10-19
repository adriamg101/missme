<!-- src/components/hierarchy-config/TlbConfigForm.vue -->
<script setup lang="ts">
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";
import NumberStepper from "../ui/NumberStepper.vue";
import SelectField from "../ui/SelectField.vue";
import ConfigGrid from "./ConfigGrid.vue";
import type { TlbConfig, TlbReplacementPolicy } from "../../lib/hierarchy/model";
import type { PageWalkTarget } from "../../lib/hierarchy/derive";

const props = defineProps<{ tlb: TlbConfig; pageWalkTargets: PageWalkTarget[] }>();
const emit = defineEmits<{ update: [patch: Partial<TlbConfig>] }>();

const pageWalkOptions = props.pageWalkTargets.map((target) => ({ value: target.id, label: target.label }));
const replacementOptions = ["LRU", "FIFO", "Random", "LFU"].map((value) => ({ value, label: value }));

function onEntriesChange(value: number) {
  const patch: Partial<TlbConfig> = { entries: value };
  if (props.tlb.associativity > value) patch.associativity = value;
  emit("update", patch);
}
</script>

<template>
  <form class="tlb-config-form" @submit.prevent>
    <ConfigGrid>
      <SelectField
        label="Page walk start"
        :model-value="tlb.pageWalkStart"
        :options="pageWalkOptions"
        aria-label="Page walk start"
        @update:model-value="emit('update', { pageWalkStart: $event })"
      />

      <PowerOfTwoStepper
        label="Entries"
        :model-value="tlb.entries"
        aria-label="Entries"
        @update:model-value="onEntriesChange"
      />

      <PowerOfTwoStepper
        label="Associativity"
        :model-value="tlb.associativity"
        :max="tlb.entries"
        aria-label="TLB associativity"
        @update:model-value="emit('update', { associativity: $event })"
      />

      <SelectField
        label="Replacement policy"
        :model-value="tlb.replacement"
        :options="replacementOptions"
        aria-label="TLB replacement policy"
        @update:model-value="emit('update', { replacement: $event as TlbReplacementPolicy })"
      />

      <NumberStepper
        label="Latency (cycles)"
        :model-value="tlb.latency"
        :min="1"
        aria-label="TLB latency"
        @update:model-value="emit('update', { latency: $event })"
      />
    </ConfigGrid>
  </form>
</template>
