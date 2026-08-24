<script setup lang="ts">
import StatsView from "./StatsView.vue";
import StatsButton from "./StatsButton.vue";
import RegistersView from "./RegistersView.vue";
import CacheLevelView from "./CacheLevelView.vue";
import RamView from "./RamView.vue";
import { useSimulationStore } from "../../stores/simulation";

const store = useSimulationStore();
</script>

<template>
  <div class="simulation-info-panel">
    <StatsView v-if="store.activeView.kind === 'stats'" />
    <RegistersView v-else-if="store.activeView.kind === 'registers'" />
    <CacheLevelView
      v-else-if="store.activeView.kind === 'cache'"
      :key="`${store.activeView.levelId}:${store.activeView.side}`"
      :levelId="store.activeView.levelId"
      :side="store.activeView.side"
    />
    <RamView v-else-if="store.activeView.kind === 'ram'" />
    <StatsButton />
  </div>
</template>

<style scoped>
.simulation-info-panel {
  position: relative;
  width: calc(100% - 0.5rem);
  height: calc(100% - 0.5rem);
  margin: 0.25rem;
  box-sizing: border-box;
  border-radius: 12px;
  border: 1px solid var(--secondary);
  background: var(--background);
  box-shadow: var(--shadow);
  overflow-y: auto;
}
</style>
