<script setup lang="ts">
import { computed, ref } from "vue";
import { computeLayout } from "../../lib/hierarchy/diagram";
import type { DiagramBox } from "../../lib/hierarchy/diagram";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const store = useHierarchyConfigStore();

const layout = computed(() => computeLayout(store.hierarchy, store.ram, store.disk, store.tlb, store.vmEnabled));

const pressedBoxId = ref<string | null>(null);

function onBoxClick(box: DiagramBox) {
  if (!box.clickable) return;
  pressedBoxId.value = box.id;
  setTimeout(() => {
    if (pressedBoxId.value === box.id) pressedBoxId.value = null;
  }, 150);
}
</script>

<template>
  <div class="hierarchy-diagram">
    <svg
      class="hierarchy-diagram-svg"
      :viewBox="`0 0 ${layout.width} ${layout.height}`"
      :width="layout.width"
      :height="layout.height"
    >
      <path v-for="connector in layout.connectors" :key="connector.id" :d="connector.path" class="hierarchy-diagram-connector" />

      <g
        v-for="box in layout.boxes"
        :key="box.id"
        :data-box-id="box.id"
        :class="['hierarchy-diagram-box', { clickable: box.clickable, pressed: pressedBoxId === box.id }]"
        @click="onBoxClick(box)"
      >
        <rect :x="box.x" :y="box.y" :width="box.width" :height="box.height" :rx="6" :fill="box.fill" class="hierarchy-diagram-box-rect" />
        <text :x="box.x + box.width / 2" :y="box.y + 18" class="hierarchy-diagram-box-label">{{ box.label }}</text>
        <text
          v-for="(sub, index) in box.subLabels"
          :key="index"
          :x="box.x + box.width / 2"
          :y="box.y + 34 + index * 12"
          class="hierarchy-diagram-box-sub"
        >{{ sub }}</text>
      </g>
    </svg>
  </div>
</template>

<style scoped>
.hierarchy-diagram {
  position: relative;
  width: calc(100% - 0.5rem);
  height: calc(100% - 0.5rem);
  margin: 0.25rem;
  padding: 0.5rem;
  box-sizing: border-box;
  border-radius: 12px;
  border: 1px solid var(--secondary);
  background: var(--background);
  box-shadow: var(--shadow);
  overflow-x: auto;
  overflow-y: hidden;
}

.hierarchy-diagram-svg {
  display: block;
}

.hierarchy-diagram-connector {
  fill: none;
  stroke: var(--secondary);
  stroke-width: 3;
}

.hierarchy-diagram-box-rect {
  stroke: var(--secondary);
  stroke-width: 1.5;
}

.hierarchy-diagram-box-label {
  text-anchor: middle;
  font-size: 11px;
  font-weight: 700;
  fill: var(--text);
  pointer-events: none;
}

.hierarchy-diagram-box-sub {
  text-anchor: middle;
  font-size: 8px;
  fill: var(--text);
  opacity: 0.65;
  pointer-events: none;
}

.hierarchy-diagram-box.clickable {
  cursor: pointer;
}

.hierarchy-diagram-box.pressed .hierarchy-diagram-box-rect {
  opacity: 0.7;
}
</style>
