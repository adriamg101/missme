<script setup lang="ts">
import MainLayout from "./components/MainLayout.vue"
import Editor from "./components/Editor.vue"
import TitleBar from "./components/TitleBar.vue"
import HierarchyConfigPanel from "./components/hierarchy-config/HierarchyConfigPanel.vue"
import HierarchyDiagram from "./components/hierarchy-config/HierarchyDiagram.vue"
import SimulationInfoPanel from "./components/simulation/SimulationInfoPanel.vue"
import { useSimulationStore } from "./stores/simulation"
import type { DiagramBox } from "./lib/hierarchy/diagram"

const simulationStore = useSimulationStore()

function handleBoxClick(box: DiagramBox) {
  if (!simulationStore.isActive) return
  if (box.kind === "reg") {
    simulationStore.openView({ kind: "registers" })
  } else if (box.kind === "cache") {
    const levelId = box.id.replace(/-[id]$/, "")
    simulationStore.openView({ kind: "cache", levelId })
  } else if (box.kind === "ram") {
    simulationStore.openView({ kind: "ram" })
  }
  // tlb/disk: no dedicated view this epic, deliberate no-op.
}
</script>

<template>
  <div class="app-shell">
    <TitleBar />
    <div class="app-content">
      <MainLayout>
        <template #left>
          <Editor></Editor>
        </template>

        <template #top-right>
          <HierarchyDiagram @box-click="handleBoxClick"></HierarchyDiagram>
        </template>

        <template #bottom-right>
          <SimulationInfoPanel v-if="simulationStore.isActive"></SimulationInfoPanel>
          <HierarchyConfigPanel v-else></HierarchyConfigPanel>
        </template>
      </MainLayout>
    </div>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}

.app-content {
  flex: 1 1 auto;
  min-height: 0;
}
</style>
