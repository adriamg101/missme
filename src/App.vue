<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue"
import { listen } from "@tauri-apps/api/event"
import MainLayout from "./components/MainLayout.vue"
import Editor from "./components/Editor.vue"
import TitleBar from "./components/TitleBar.vue"
import HierarchyConfigPanel from "./components/hierarchy-config/HierarchyConfigPanel.vue"
import HierarchyDiagram from "./components/hierarchy-config/HierarchyDiagram.vue"
import SimulationInfoPanel from "./components/simulation/SimulationInfoPanel.vue"
import { useSimulationStore } from "./stores/simulation"
import { useHierarchyConfigStore } from "./stores/hierarchyConfig"
import type { DiagramBox } from "./lib/hierarchy/diagram"

const simulationStore = useSimulationStore()
const hierarchyConfigStore = useHierarchyConfigStore()
const diagramRef = ref<InstanceType<typeof HierarchyDiagram> | null>(null)

function levelIdAndSideFromBoxId(boxId: string): { levelId: string; side: "unified" | "i" | "d" } {
  if (boxId.endsWith("-i")) return { levelId: boxId.slice(0, -2), side: "i" }
  if (boxId.endsWith("-d")) return { levelId: boxId.slice(0, -2), side: "d" }
  return { levelId: boxId, side: "unified" }
}

function handleBoxClick(box: DiagramBox) {
  if (!simulationStore.isActive) return
  if (box.kind === "reg") {
    simulationStore.openView({ kind: "registers" })
  } else if (box.kind === "cache") {
    const { levelId, side } = levelIdAndSideFromBoxId(box.id)
    simulationStore.openView({ kind: "cache", levelId, side })
  } else if (box.kind === "ram") {
    simulationStore.openView({ kind: "ram" })
  } else if (box.kind === "pt") {
    simulationStore.openView({ kind: "pageTable" })
  } else if (box.kind === "frames") {
    simulationStore.openView({ kind: "frames" })
  } else if (box.kind === "tlb") {
    simulationStore.openView({ kind: "tlb" })
  } else if (box.kind === "disk") {
    simulationStore.openView({ kind: "disk" })
  }
}

function boxIdsForLevel(levelIndex: number, accessKind: "fetch" | "load" | "store"): string[] {
  const level = hierarchyConfigStore.hierarchy[levelIndex]
  if (!level) return []
  if (!level.harvard) return [level.id]
  return [accessKind === "fetch" ? `${level.id}-i` : `${level.id}-d`]
}

let pendingClear = false

function handleSimEvent(event: { payload: Record<string, unknown> }) {
  const kind = event.payload.kind as string

  if (pendingClear && kind !== "instruction_complete") {
    diagramRef.value?.clearAccessStates()
    pendingClear = false
  }

  if (kind === "cache_access") {
    const levelIndex = event.payload.levelIndex as number
    const outcome = event.payload.outcome as "hit" | "miss"
    const accessKind = event.payload.accessKind as "fetch" | "load" | "store"
    for (const boxId of boxIdsForLevel(levelIndex, accessKind)) {
      diagramRef.value?.triggerHitMiss(boxId, outcome)
    }
    diagramRef.value?.setStatusMessage(event.payload.accessKind as "fetch" | "load" | "store", event.payload.address as number)
  } else if (kind === "ram_access") {
    diagramRef.value?.setStatusMessage(event.payload.accessKind as "fetch" | "load" | "store", event.payload.address as number)
  } else if (kind === "disk_access") {
    diagramRef.value?.setStatusMessage(event.payload.accessKind as "fetch" | "load" | "store", event.payload.address as number)
  } else if (kind === "tlb_access") {
    const outcome = event.payload.outcome as "hit" | "miss"
    diagramRef.value?.triggerHitMiss("tlb", outcome)
    diagramRef.value?.setStatusMessage(event.payload.accessKind as "fetch" | "load" | "store", event.payload.address as number)
  } else if (kind === "instruction_complete") {
    pendingClear = true
  }
}

// This clears the diagram colors and message once the run ends, even if the program just finishes on its own instead of the user pressing Stop.
watch(
  () => simulationStore.isActive,
  (active) => {
    if (!active) {
      diagramRef.value?.clearAccessStates()
      pendingClear = false
    }
  },
)

let unlistenSimEvent: (() => void) | null = null
let disposed = false

onMounted(async () => {
  const stopListening = await listen<Record<string, unknown>>("sim:event", handleSimEvent)
  if (disposed) stopListening()
  else unlistenSimEvent = stopListening
})

onBeforeUnmount(() => {
  disposed = true
  unlistenSimEvent?.()
})
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
          <HierarchyDiagram ref="diagramRef" @box-click="handleBoxClick"></HierarchyDiagram>
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
