<!-- src/components/hierarchy-config/HierarchySlider.vue -->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { Hierarchy, ISA } from "../../lib/hierarchy/model";
import { maxPositions } from "../../lib/hierarchy/model";
import { isPositionInsertable, levelName, occupiedSpans, sizeLabel } from "../../lib/hierarchy/derive";
import { MIN_POSITION } from "../../lib/hierarchy/operations";

const props = defineProps<{ hierarchy: Hierarchy; isa: ISA }>();

const emit = defineEmits<{
  create: [position: number];
  "rejected-insert": [levelIds: string[]];
  move: [levelId: string, chain: "i" | "d" | "unified", position: number];
  "convert-to-harvard": [levelId: string];
  "convert-to-von-neumann": [levelId: string];
  "remove-level": [levelId: string];
}>();

// The reachable range is [MIN_POSITION, maxPositions(isa) - 1] — MIN_POSITION
// is 0, so a cache with a single entry (2^0 = 1) is reachable, same as any
// other power-of-two position.
const maxPosition = computed(() => maxPositions(props.isa) - 1);
const positions = computed(() =>
  Array.from({ length: maxPosition.value - MIN_POSITION + 1 }, (_, i) => i + MIN_POSITION),
);

// Label every position whose value is itself a power of two, plus position
// 0 (2^0 = 1, not reachable by doubling from 1) and the maximum reachable
// position (which is only a power of two for RV64I).
const labeledPositions = computed<Set<number>>(() => {
  const set = new Set<number>([0]);
  for (let value = 1; value <= maxPosition.value; value *= 2) set.add(value);
  set.add(maxPosition.value);
  return set;
});

interface Marker {
  levelId: string;
  levelIndex: number;
  chain: "i" | "d" | "unified";
  exp: number;
  ariaLabel: string;
  labelText: string;
  indexLabel: string;
  side: "above" | "below";
}

const markers = computed<Marker[]>(() => {
  const result: Marker[] = [];
  props.hierarchy.forEach((level, index) => {
    if (level.harvard) {
      result.push({
        levelId: level.id,
        levelIndex: index,
        chain: "i",
        exp: level.i!.exp,
        labelText: sizeLabel(level.i!.exp),
        indexLabel: `${index + 1}i`,
        ariaLabel: `${levelName(props.hierarchy, index)} instrucciones, ${sizeLabel(level.i!.exp)}`,
        side: "above",
      });
      result.push({
        levelId: level.id,
        levelIndex: index,
        chain: "d",
        exp: level.d!.exp,
        labelText: sizeLabel(level.d!.exp),
        indexLabel: `${index + 1}d`,
        ariaLabel: `${levelName(props.hierarchy, index)} datos, ${sizeLabel(level.d!.exp)}`,
        side: "below",
      });
    } else {
      result.push({
        levelId: level.id,
        levelIndex: index,
        chain: "unified",
        exp: level.unified!.exp,
        labelText: sizeLabel(level.unified!.exp),
        indexLabel: `${index + 1}`,
        ariaLabel: `${levelName(props.hierarchy, index)}, ${sizeLabel(level.unified!.exp)}`,
        side: "above",
      });
    }
  });
  return result;
});

function onPositionClick(position: number) {
  if (isPositionInsertable(props.hierarchy, position)) {
    emit("create", position);
    return;
  }
  const blocking = occupiedSpans(props.hierarchy)
    .filter((span) => position >= span.from && position <= span.to)
    .map((span) => props.hierarchy[span.levelIndex].id);
  emit("rejected-insert", blocking);
}

const DRAG_THRESHOLD_PX = 4;
// Fallback when the axis hasn't been laid out yet (e.g. jsdom, which always
// reports a zero-size getBoundingClientRect) — real browsers use the
// measured track width instead.
const FALLBACK_PIXELS_PER_POSITION = 20;
const axisEl = ref<HTMLElement | null>(null);

let dragState: {
  levelId: string;
  chain: "i" | "d" | "unified";
  startX: number;
  startExp: number;
  dragging: boolean;
  pixelsPerPosition: number;
} | null = null;

function onMarkerMouseDown(levelId: string, chain: "i" | "d" | "unified", startExp: number, event: MouseEvent) {
  if (event.button !== 0) return;
  const axisWidth = axisEl.value?.getBoundingClientRect().width ?? 0;
  const range = maxPosition.value - MIN_POSITION;
  const pixelsPerPosition = axisWidth > 0 && range > 0 ? axisWidth / range : FALLBACK_PIXELS_PER_POSITION;
  dragState = { levelId, chain, startX: event.clientX, startExp, dragging: false, pixelsPerPosition };
  window.addEventListener("mousemove", onWindowMouseMove);
  window.addEventListener("mouseup", onWindowMouseUp);
}

function onWindowMouseMove(event: MouseEvent) {
  if (!dragState) return;
  const deltaPx = event.clientX - dragState.startX;
  if (!dragState.dragging && Math.abs(deltaPx) < DRAG_THRESHOLD_PX) return;
  dragState.dragging = true;
  const deltaPositions = Math.round(deltaPx / dragState.pixelsPerPosition);
  emit("move", dragState.levelId, dragState.chain, dragState.startExp + deltaPositions);
}

function onWindowMouseUp() {
  window.removeEventListener("mousemove", onWindowMouseMove);
  window.removeEventListener("mouseup", onWindowMouseUp);
  dragState = null;
}

// Positioned in fixed viewport coordinates and teleported to <body> so the
// menu is never clipped by the panel's own overflow:hidden card chrome.
const contextMenu = ref<{ levelId: string; label: string; harvard: boolean; x: number; y: number } | null>(null);
const contextMenuEl = ref<HTMLElement | null>(null);

function openContextMenu(marker: Marker, event: MouseEvent) {
  event.preventDefault();
  const index = props.hierarchy.findIndex((l) => l.id === marker.levelId);
  if (index === -1) return;
  contextMenu.value = {
    levelId: marker.levelId,
    label: levelName(props.hierarchy, index),
    harvard: props.hierarchy[index].harvard,
    x: event.clientX,
    y: event.clientY,
  };
}

function closeContextMenu() {
  contextMenu.value = null;
}

function chooseHarvard() {
  if (!contextMenu.value) return;
  emit("convert-to-harvard", contextMenu.value.levelId);
  closeContextMenu();
}

function chooseVonNeumann() {
  if (!contextMenu.value) return;
  emit("convert-to-von-neumann", contextMenu.value.levelId);
  closeContextMenu();
}

function chooseRemove() {
  if (!contextMenu.value) return;
  emit("remove-level", contextMenu.value.levelId);
  closeContextMenu();
}

function onGlobalKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") closeContextMenu();
}

function handleDocumentClick(event: MouseEvent) {
  if (!contextMenu.value) return;
  if (contextMenuEl.value && !contextMenuEl.value.contains(event.target as Node)) {
    closeContextMenu();
  }
}

function onMarkerKeydown(marker: Marker, event: KeyboardEvent) {
  if (event.key === "ArrowRight") emit("move", marker.levelId, marker.chain, marker.exp + 1);
  if (event.key === "ArrowLeft") emit("move", marker.levelId, marker.chain, marker.exp - 1);
  if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
    event.preventDefault();
    const index = props.hierarchy.findIndex((l) => l.id === marker.levelId);
    if (index === -1) return;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.value = {
      levelId: marker.levelId,
      label: levelName(props.hierarchy, index),
      harvard: props.hierarchy[index].harvard,
      x: rect.left + rect.width / 2,
      y: rect.bottom,
    };
  }
}

onMounted(() => {
  document.addEventListener("keydown", onGlobalKeydown);
  document.addEventListener("mousedown", handleDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", onGlobalKeydown);
  document.removeEventListener("mousedown", handleDocumentClick);
});
</script>

<template>
  <div class="hierarchy-slider">
    <span class="hierarchy-slider-title">Number of entries (tags)</span>
    <div
      ref="axisEl"
      class="hierarchy-slider-axis"
      :style="{ '--min': MIN_POSITION, '--total': maxPosition - MIN_POSITION }"
    >
      <div
        v-for="p in positions"
        :key="p"
        data-slider-position
        class="hierarchy-slider-tick"
        :style="{ left: ((p - MIN_POSITION) / (maxPosition - MIN_POSITION)) * 100 + '%' }"
        @click="onPositionClick(p)"
      >
        <span
          v-if="labeledPositions.has(p)"
          class="hierarchy-slider-tick-label"
        >{{ p }}</span>
      </div>

      <button
        v-for="marker in markers"
        :key="`${marker.levelId}-${marker.chain}`"
        type="button"
        class="hierarchy-slider-marker"
        :class="[
          `marker-${marker.side}`,
          `marker-${marker.chain}`,
          { 'is-selected': contextMenu?.levelId === marker.levelId },
        ]"
        :style="{ '--position': marker.exp }"
        :aria-label="marker.ariaLabel"
        tabindex="0"
        @mousedown="onMarkerMouseDown(marker.levelId, marker.chain, marker.exp, $event)"
        @contextmenu="openContextMenu(marker, $event)"
        @keydown="onMarkerKeydown(marker, $event)"
      >
        <span class="hierarchy-slider-marker-index">{{ marker.indexLabel }}</span>
        <span class="hierarchy-slider-marker-label">{{ marker.labelText }}</span>
      </button>
    </div>

    <Teleport to="body">
      <div
        v-if="contextMenu"
        ref="contextMenuEl"
        class="hierarchy-slider-context-menu"
        :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
      >
        <button v-if="!contextMenu.harvard" type="button" @click="chooseHarvard">
          Convertir {{ contextMenu.label }} a Harvard
        </button>
        <button v-else type="button" @click="chooseVonNeumann">
          Convertir {{ contextMenu.label }} a Von Neumann
        </button>
        <button type="button" @click="chooseRemove">Eliminar {{ contextMenu.label }}</button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.hierarchy-slider {
  position: relative;
  width: 100%;
}

.hierarchy-slider-title {
  display: block;
  padding: 0 1.5rem;
  font-size: 0.875rem;
  color: var(--text);
}

.hierarchy-slider-axis {
  position: relative;
  height: 6.5rem;
  /* Half a marker's own width/height plus room for its focus outline and
     the is-selected box-shadow, so an edge marker never gets clipped by
     the panel's overflow:hidden card chrome. */
  margin: 0 1.5rem;
}

.hierarchy-slider-axis::before {
  content: "";
  position: absolute;
  top: 50%;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--secondary);
  transform: translateY(-50%);
}

.hierarchy-slider-tick {
  position: absolute;
  top: 50%;
  transform: translate(-50%, -50%);
  width: 1.5rem;
  height: 1.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.hierarchy-slider-tick::before {
  content: "";
  width: 1px;
  height: 8px;
  background: var(--secondary);
}

.hierarchy-slider-tick:hover::before {
  background: var(--accent);
}

.hierarchy-slider-tick-label {
  position: absolute;
  top: calc(100% + 2px);
  left: 50%;
  transform: translateX(-50%);
  font-size: 0.65rem;
  color: var(--text);
  opacity: 0.6;
  white-space: nowrap;
  pointer-events: none;
}

.hierarchy-slider-marker {
  position: absolute;
  top: 50%;
  left: calc((var(--position) - var(--min)) / var(--total) * 100%);
  transform: translate(-50%, -50%);
  width: 1.75rem;
  height: 1.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  border: 2px solid var(--accent);
  background: var(--background);
  padding: 0;
  cursor: grab;
  z-index: 5;
}

/* Harvard i/d markers stack vertically (one above the other) instead of
   sharing the track's center line, so they read as a pair even when they
   sit at different horizontal positions. */
.hierarchy-slider-marker.marker-i {
  top: 32%;
}

.hierarchy-slider-marker.marker-d {
  top: 68%;
  border-color: var(--primary);
}

.hierarchy-slider-marker-index {
  font-size: 0.65rem;
  font-weight: 700;
  color: var(--text);
  pointer-events: none;
}

.hierarchy-slider-marker:hover {
  background: var(--accent);
}

.hierarchy-slider-marker.is-selected {
  background: var(--accent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent);
}

.hierarchy-slider-marker:active {
  cursor: grabbing;
}

.hierarchy-slider-marker:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.hierarchy-slider-marker-label {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  font-size: 0.7rem;
  color: var(--text);
  white-space: nowrap;
  pointer-events: none;
}

.hierarchy-slider-marker.marker-above .hierarchy-slider-marker-label {
  bottom: calc(100% + 6px);
}

.hierarchy-slider-marker.marker-below .hierarchy-slider-marker-label {
  top: calc(100% + 6px);
}

.hierarchy-slider-context-menu {
  position: fixed;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  padding: 0.5rem;
  background: var(--background);
  border: 1px solid var(--secondary);
  border-radius: 8px;
  box-shadow: var(--shadow);
  z-index: 1000;
}

.hierarchy-slider-context-menu button {
  min-height: 1.5rem;
  padding: 0.5rem 0.75rem;
  border: none;
  border-radius: 4px;
  background: none;
  color: var(--text);
  font: inherit;
  text-align: left;
  white-space: nowrap;
  cursor: pointer;
}

.hierarchy-slider-context-menu button:hover,
.hierarchy-slider-context-menu button:focus-visible {
  background: var(--secondary);
  color: var(--background);
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
</style>
