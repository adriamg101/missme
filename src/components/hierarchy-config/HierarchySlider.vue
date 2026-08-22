<!-- src/components/hierarchy-config/HierarchySlider.vue -->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { Hierarchy, ISA } from "../../lib/hierarchy/model";
import { maxPositions } from "../../lib/hierarchy/model";
import { chainLabel, isPositionInsertable, levelName, occupiedSpans, sizeLabel } from "../../lib/hierarchy/derive";

const props = defineProps<{ hierarchy: Hierarchy; isa: ISA }>();

const emit = defineEmits<{
  create: [position: number];
  "rejected-insert": [levelIds: string[]];
  move: [levelId: string, chain: "i" | "d" | "unified", position: number];
  "convert-to-harvard": [levelId: string];
  "convert-to-von-neumann": [levelId: string];
  "remove-level": [levelId: string];
}>();

const positionCount = computed(() => maxPositions(props.isa));
const positions = computed(() => Array.from({ length: positionCount.value }, (_, i) => i));
const labelEvery = computed(() => (positionCount.value > 32 ? 4 : 1));

const spans = computed(() => occupiedSpans(props.hierarchy));

// Positions already carrying a marker's size label must not also get an axis
// tick label at the same position, or the two would render identical text.
const markerPositions = computed<Set<number>>(() => {
  const set = new Set<number>();
  props.hierarchy.forEach((level) => {
    if (level.harvard) {
      set.add(level.i!.exp);
      set.add(level.d!.exp);
    } else {
      set.add(level.unified!.exp);
    }
  });
  return set;
});

interface Marker {
  levelId: string;
  levelIndex: number;
  chain: "i" | "d" | "unified";
  exp: number;
  ariaLabel: string;
  labelText: string;
  side: "above" | "below";
}

const markers = computed<Marker[]>(() => {
  const result: Marker[] = [];
  props.hierarchy.forEach((level, index) => {
    if (level.harvard) {
      const iLabel = chainLabel(props.hierarchy, index, "i");
      const dLabel = chainLabel(props.hierarchy, index, "d");
      result.push({
        levelId: level.id,
        levelIndex: index,
        chain: "i",
        exp: level.i!.exp,
        labelText: sizeLabel(level.i!.exp),
        ariaLabel: `${levelName(props.hierarchy, index)} instrucciones, ${sizeLabel(level.i!.exp)}`,
        side: "above",
      });
      result.push({
        levelId: level.id,
        levelIndex: index,
        chain: "d",
        exp: level.d!.exp,
        labelText: sizeLabel(level.d!.exp),
        ariaLabel: `${levelName(props.hierarchy, index)} datos, ${sizeLabel(level.d!.exp)}`,
        side: "below",
      });
      // keep iLabel/dLabel available for the tab strip via chainLabel; unused locally beyond aria text
      void iLabel;
      void dLabel;
    } else {
      result.push({
        levelId: level.id,
        levelIndex: index,
        chain: "unified",
        exp: level.unified!.exp,
        labelText: sizeLabel(level.unified!.exp),
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
const PIXELS_PER_POSITION = 20; // matches the axis's CSS custom property step

let dragState: { levelId: string; chain: "i" | "d" | "unified"; startX: number; startExp: number; dragging: boolean } | null = null;

function onMarkerMouseDown(levelId: string, chain: "i" | "d" | "unified", startExp: number, event: MouseEvent) {
  if (event.button !== 0) return;
  dragState = { levelId, chain, startX: event.clientX, startExp, dragging: false };
  window.addEventListener("mousemove", onWindowMouseMove);
  window.addEventListener("mouseup", onWindowMouseUp);
}

function onWindowMouseMove(event: MouseEvent) {
  if (!dragState) return;
  const deltaPx = event.clientX - dragState.startX;
  if (!dragState.dragging && Math.abs(deltaPx) < DRAG_THRESHOLD_PX) return;
  dragState.dragging = true;
  const deltaPositions = Math.round(deltaPx / PIXELS_PER_POSITION);
  emit("move", dragState.levelId, dragState.chain, dragState.startExp + deltaPositions);
}

function onWindowMouseUp() {
  window.removeEventListener("mousemove", onWindowMouseMove);
  window.removeEventListener("mouseup", onWindowMouseUp);
  dragState = null;
}

const contextMenu = ref<{ levelId: string; label: string; harvard: boolean } | null>(null);
const contextMenuEl = ref<HTMLElement | null>(null);

function openContextMenu(levelId: string, event: MouseEvent) {
  event.preventDefault();
  const index = props.hierarchy.findIndex((l) => l.id === levelId);
  if (index === -1) return;
  contextMenu.value = { levelId, label: levelName(props.hierarchy, index), harvard: props.hierarchy[index].harvard };
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
    contextMenu.value = { levelId: marker.levelId, label: levelName(props.hierarchy, index), harvard: props.hierarchy[index].harvard };
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
    <div class="hierarchy-slider-axis">
      <div
        v-for="p in positions"
        :key="p"
        data-slider-position
        class="hierarchy-slider-tick"
        @click="onPositionClick(p)"
      >
        <span
          v-if="p % labelEvery === 0 && !markerPositions.has(p)"
          class="hierarchy-slider-tick-label"
        >{{ sizeLabel(p) }}</span>
      </div>

      <div
        v-for="span in spans"
        :key="`span-${span.levelIndex}`"
        data-occupied-span
        class="hierarchy-slider-span"
        :style="{ '--from': span.from, '--to': span.to }"
      ></div>

      <button
        v-for="marker in markers"
        :key="`${marker.levelId}-${marker.chain}`"
        type="button"
        class="hierarchy-slider-marker"
        :class="[`marker-${marker.side}`]"
        :style="{ '--position': marker.exp }"
        :aria-label="marker.ariaLabel"
        tabindex="0"
        @mousedown="onMarkerMouseDown(marker.levelId, marker.chain, marker.exp, $event)"
        @contextmenu="openContextMenu(marker.levelId, $event)"
        @keydown="onMarkerKeydown(marker, $event)"
      >
        <span class="hierarchy-slider-marker-label">{{ marker.labelText }}</span>
      </button>
    </div>

    <div v-if="contextMenu" ref="contextMenuEl" class="hierarchy-slider-context-menu">
      <button v-if="!contextMenu.harvard" type="button" @click="chooseHarvard">
        Convertir {{ contextMenu.label }} a Harvard
      </button>
      <button v-else type="button" @click="chooseVonNeumann">
        Convertir {{ contextMenu.label }} a Von Neumann
      </button>
      <button type="button" @click="chooseRemove">Eliminar {{ contextMenu.label }}</button>
    </div>
  </div>
</template>
