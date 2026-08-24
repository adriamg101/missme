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
      :key="store.activeView.levelId"
      :levelId="store.activeView.levelId"
    />
    <RamView v-else-if="store.activeView.kind === 'ram'" />
    <StatsButton />
  </div>
</template>

<style scoped>
.simulation-info-panel {
  position: relative;
  height: 100%;
  overflow-y: auto;
}
</style>
