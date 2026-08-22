<!-- src/components/hierarchy-config/GlobalConfigBar.vue -->
<script setup lang="ts">
import { computed } from "vue";
import HierarchySlider from "./HierarchySlider.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { levelName } from "../../lib/hierarchy/derive";

const store = useHierarchyConfigStore();

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
      `No se puede convertir a Von Neumann: entraría en conflicto con ${levelName(store.hierarchy, result.conflictIndex)}.`,
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

    <div class="global-config-bar-row">
      <label>
        <input type="checkbox" aria-label="Virtual Memory" :checked="store.vmEnabled" @change="store.setVmEnabled(($event.target as HTMLInputElement).checked)" />
        Virtual Memory
      </label>

      <input type="number" placeholder="fully random" v-model="seedText" />
    </div>
  </div>
</template>
