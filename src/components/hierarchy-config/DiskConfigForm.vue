<!-- src/components/hierarchy-config/DiskConfigForm.vue -->
<script setup lang="ts">
import NumberStepper from "../ui/NumberStepper.vue";
import ConfigGrid from "./ConfigGrid.vue";
import type { DiskConfig } from "../../lib/hierarchy/model";

defineProps<{ config: DiskConfig }>();
const emit = defineEmits<{ update: [patch: Partial<DiskConfig>] }>();
</script>

<template>
  <form class="disk-config-form" @submit.prevent>
    <ConfigGrid>
      <NumberStepper
        label="Latency (cycles)"
        :model-value="config.latency"
        :min="1"
        aria-label="Disk latency"
        @update:model-value="emit('update', { latency: $event })"
      />
    </ConfigGrid>
  </form>
</template>
