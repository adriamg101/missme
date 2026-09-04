<script setup lang="ts">
import StatsView from "./StatsView.vue";
import StatsButton from "./StatsButton.vue";
import RegistersView from "./RegistersView.vue";
import CacheLevelView from "./CacheLevelView.vue";
import RamView from "./RamView.vue";
import TlbView from "./TlbView.vue";
import DiskView from "./DiskView.vue";
import FramesView from "./FramesView.vue";
import PageTableView from "./PageTableView.vue";
import { useSimulationStore } from "../../stores/simulation";

const store = useSimulationStore();
</script>

<template>
  <div class="simulation-info-panel">
    <div v-if="store.activeView.kind !== 'stats'" class="simulation-info-panel-topbar">
      <StatsButton />
    </div>
    <div class="simulation-info-panel-content">
      <StatsView v-if="store.activeView.kind === 'stats'" />
      <RegistersView v-else-if="store.activeView.kind === 'registers'" />
      <CacheLevelView
        v-else-if="store.activeView.kind === 'cache'"
        :key="`${store.activeView.levelId}:${store.activeView.side}`"
        :levelId="store.activeView.levelId"
        :side="store.activeView.side"
      />
      <RamView v-else-if="store.activeView.kind === 'ram'" />
      <TlbView v-else-if="store.activeView.kind === 'tlb'" />
      <DiskView v-else-if="store.activeView.kind === 'disk'" />
      <FramesView v-else-if="store.activeView.kind === 'frames'" />
      <PageTableView v-else-if="store.activeView.kind === 'pageTable'" />
    </div>
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
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.simulation-info-panel-topbar {
  flex: 0 0 auto;
  display: flex;
  justify-content: center;
  padding: 0.4rem;
  border-bottom: 1px solid var(--secondary);
}

.simulation-info-panel-content {
  flex: 1 1 auto;
  min-height: 0;
  position: relative;
}
</style>
