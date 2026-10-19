<!-- src/components/hierarchy-config/GlobalConfigBar.vue -->
<script setup lang="ts">
import { computed } from "vue";
import HierarchySlider from "./HierarchySlider.vue";
import TextField from "../ui/TextField.vue";
import ConfigGrid from "./ConfigGrid.vue";
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { levelName } from "../../lib/hierarchy/derive";

const store = useHierarchyConfigStore();

// Keeps the page size from getting too small to be realistic or too large for the backend to accept.
const MIN_PAGE_SIZE = 4;
const MAX_PAGE_SIZE = 65536;

const seedText = computed({
  get: () => (store.seed === null ? "" : String(store.seed)),
  set: (value: string) => {
    store.setSeed(value === "" ? null : Number(value));
  },
});

function onConvertToVonNeumann(levelId: string) {
  const result = store.convertToVonNeumann(levelId);
  if (!result.ok) {
    alert(
      `Cannot convert to Von Neumann: it would conflict with ${levelName(store.hierarchy, result.conflictIndex)}.`,
    );
  }
}
</script>

<template>
  <div class="global-config-bar">
    <HierarchySlider
      :hierarchy="store.hierarchy"
      :isa="store.isa"
      @create="store.createLevelAt"
      @move="(levelId, chain, position) => store.moveMarker(levelId, chain, position)"
      @convert-to-harvard="store.convertToHarvard"
      @convert-to-von-neumann="onConvertToVonNeumann"
      @remove-level="store.removeLevel"
    />

    <ConfigGrid class="global-config-bar-row">
      <div class="global-config-bar-field">
        <span class="global-config-bar-field-label">Virtual Memory</span>
        <input
          type="checkbox"
          aria-label="Virtual Memory"
          :checked="store.vmEnabled"
          @change="store.setVmEnabled(($event.target as HTMLInputElement).checked)"
        />
      </div>

      <PowerOfTwoStepper
        v-if="store.vmEnabled"
        label="Page size"
        aria-label="Page size"
        :model-value="store.pageSize"
        :min="MIN_PAGE_SIZE"
        :max="MAX_PAGE_SIZE"
        @update:model-value="store.setPageSize"
      />

      <TextField type="number" label="Seed" placeholder="fully random" aria-label="Seed" v-model="seedText" />
    </ConfigGrid>
  </div>
</template>

<style scoped>
.global-config-bar {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.global-config-bar-row {
  padding: 0 0.875rem;
}

.global-config-bar-field {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
  font-size: 0.875rem;
  color: var(--text);
  transition: color var(--theme-transition-duration) ease;
}

.global-config-bar-field input[type="checkbox"] {
  width: 1.25rem;
  height: 1.25rem;
  accent-color: var(--accent);
}
</style>
