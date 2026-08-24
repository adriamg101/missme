<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { computeLayout } from "../../lib/hierarchy/diagram";
import type { DiagramBox } from "../../lib/hierarchy/diagram";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const store = useHierarchyConfigStore();

const emit = defineEmits<{ boxClick: [box: DiagramBox] }>();

const layout = computed(() => computeLayout(store.hierarchy, store.ram, store.disk, store.tlb, store.vmEnabled));

const pressedBoxId = ref<string | null>(null);

function onBoxClick(box: DiagramBox) {
  if (!box.clickable) return;
  pressedBoxId.value = box.id;
  setTimeout(() => {
    if (pressedBoxId.value === box.id) pressedBoxId.value = null;
  }, 150);
  emit("boxClick", box);
}

type HitMiss = "hit" | "miss";
type ReadWrite = "read" | "write";
interface AccessState {
  hitMiss: HitMiss | null;
  readWrite: ReadWrite | null;
}

const accessStates = reactive<Record<string, AccessState>>({});

function stateFor(boxId: string): AccessState {
  if (!accessStates[boxId]) accessStates[boxId] = { hitMiss: null, readWrite: null };
  return accessStates[boxId];
}

// State changes, not timed effects: a box switches from its normal type
// color to the hit/miss border and stays there until the (future)
// simulation step controller calls clearAccessStates() before applying the
// next step's state — this is a step-through UI, not an auto-reverting
// flash. The visual "pulse" is just the CSS transition between the two
// states, not a self-clearing animation.
function triggerHitMiss(boxId: string, kind: HitMiss) {
  stateFor(boxId).hitMiss = kind;
}

// Instant mode: no transition, so many fast simulation ticks can toggle
// this without a visible lag — still a persistent state, cleared the same
// way as hit/miss.
function triggerReadWrite(boxId: string, kind: ReadWrite) {
  stateFor(boxId).readWrite = kind;
}

// Reverts every box to its normal type color — call this before the next
// step's triggerHitMiss/triggerReadWrite so the previous step's highlight
// doesn't linger.
function clearAccessStates() {
  for (const state of Object.values(accessStates)) {
    state.hitMiss = null;
    state.readWrite = null;
  }
}

// No caller exists yet (there's no simulation engine driving real access
// events) — exposed so a future one can trigger these without touching this
// component again.
defineExpose({ triggerHitMiss, triggerReadWrite, clearAccessStates });
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
        :style="{ '--box-hue': box.hue, '--box-chroma': box.chroma }"
        :class="[
          'hierarchy-diagram-box',
          {
            clickable: box.clickable,
            pressed: pressedBoxId === box.id,
            'access-hit': accessStates[box.id]?.hitMiss === 'hit',
            'access-miss': accessStates[box.id]?.hitMiss === 'miss',
          },
        ]"
        @click="onBoxClick(box)"
      >
        <rect :x="box.x" :y="box.y" :width="box.width" :height="box.height" :rx="6" class="hierarchy-diagram-box-rect" />
        <text :x="box.x + box.width / 2" :y="box.y + 21" class="hierarchy-diagram-box-label">{{ box.label }}</text>
        <text
          v-for="(sub, index) in box.subLabels"
          :key="index"
          :x="box.x + box.width / 2"
          :y="box.y + 38 + index * 14"
          class="hierarchy-diagram-box-sub"
        >{{ sub }}</text>
        <circle
          v-if="accessStates[box.id]?.readWrite"
          :cx="box.x + box.width - 8"
          :cy="box.y + 8"
          r="6"
          :class="['hierarchy-diagram-box-dot', accessStates[box.id]?.readWrite === 'read' ? 'dot-read' : 'dot-write']"
        />
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
  overflow-y: auto;
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
  stroke: oklch(var(--diagram-lightness-border) var(--box-chroma) var(--box-hue));
  stroke-width: 1.5;
  fill: oklch(var(--diagram-lightness-fill) calc(var(--box-chroma) * var(--diagram-fill-chroma-mult)) var(--box-hue));
  transition: stroke 0.2s ease, filter 0.2s ease;
}

.hierarchy-diagram-box-label,
.hierarchy-diagram-box-sub {
  text-anchor: middle;
  pointer-events: none;
  fill: oklch(var(--diagram-lightness-text) calc(var(--box-chroma) * var(--diagram-text-chroma-mult)) var(--box-hue));
}

.hierarchy-diagram-box-label {
  font-size: 15px;
  font-weight: 700;
}

.hierarchy-diagram-box-sub {
  font-size: 11px;
}

.hierarchy-diagram-box.clickable {
  cursor: pointer;
}

.hierarchy-diagram-box.pressed .hierarchy-diagram-box-rect {
  opacity: 0.7;
}

/* Hit/miss: a state change, not a timed effect — border + glow switch to
   the outcome color and stay there until clearAccessStates() runs (the
   future step controller calls it right before the next step). The
   "pulse" is just the transition above animating the switch, not a
   self-reverting keyframe animation. */
.hierarchy-diagram-box.access-hit .hierarchy-diagram-box-rect {
  stroke: var(--diagram-hit-color);
  filter: drop-shadow(0 0 6px var(--diagram-hit-color));
}

.hierarchy-diagram-box.access-miss .hierarchy-diagram-box-rect {
  stroke: var(--diagram-miss-color);
  filter: drop-shadow(0 0 6px var(--diagram-miss-color));
}

/* Read/write: a small corner dot, shown/hidden with no transition — the
   "instant" animation mode, for driving many fast simulation ticks. */
.hierarchy-diagram-box-dot {
  stroke: var(--background);
  stroke-width: 2;
}

.hierarchy-diagram-box-dot.dot-read {
  fill: var(--diagram-read-color);
}

.hierarchy-diagram-box-dot.dot-write {
  fill: var(--diagram-write-color);
}
</style>
