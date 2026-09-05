<script setup lang="ts">
import { ref, onBeforeUnmount } from "vue";

const MIN_PERCENT = 25;
const MAX_PERCENT = 75;

const leftPercent = ref(100 / 3);
const topPercent = ref(25);

const containerRef = ref<HTMLElement | null>(null);
const rightColumnRef = ref<HTMLElement | null>(null);

let dragging: "vertical" | "horizontal" | null = null;

function clamp(value: number): number {
  return Math.min(MAX_PERCENT, Math.max(MIN_PERCENT, value));
}

function startVerticalDrag() {
  dragging = "vertical";
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", stopDrag);
}

function startHorizontalDrag() {
  dragging = "horizontal";
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", stopDrag);
}

function onPointerMove(event: PointerEvent) {
  if (dragging === "vertical" && containerRef.value) {
    const rect = containerRef.value.getBoundingClientRect();
    leftPercent.value = clamp(((event.clientX - rect.left) / rect.width) * 100);
  } else if (dragging === "horizontal" && rightColumnRef.value) {
    const rect = rightColumnRef.value.getBoundingClientRect();
    topPercent.value = clamp(((event.clientY - rect.top) / rect.height) * 100);
  }
}

function stopDrag() {
  dragging = null;
  window.removeEventListener("pointermove", onPointerMove);
  window.removeEventListener("pointerup", stopDrag);
}

onBeforeUnmount(stopDrag);
</script>

<template>
  <div class="main-layout" ref="containerRef">
    <div class="pane pane-left" :style="{ flexBasis: leftPercent + '%' }">
      <slot name="left"></slot>
    </div>

    <div class="resize-handle resize-handle-vertical" @pointerdown="startVerticalDrag"></div>

    <div class="pane-right" ref="rightColumnRef" :style="{ flexBasis: (100 - leftPercent) + '%' }">
      <div class="pane pane-top-right" :style="{ flexBasis: topPercent + '%' }">
        <slot name="top-right"></slot>
      </div>

      <div class="resize-handle resize-handle-horizontal" @pointerdown="startHorizontalDrag"></div>

      <div class="pane pane-bottom-right" :style="{ flexBasis: (100 - topPercent) + '%' }">
        <slot name="bottom-right"></slot>
      </div>
    </div>
  </div>
</template>

<style scoped>
.main-layout {
  display: flex;
  width: 100%;
  height: 100%;
  background: var(--background);
  color: var(--text);
  transition: background-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}

.pane-left {
  flex-grow: 0;
  flex-shrink: 0;
  transition: none;
}

.pane-right {
  flex-grow: 0;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  min-width: 0;
  transition: none;
}

.pane-top-right {
  flex-grow: 0;
  flex-shrink: 0;
  transition: none;
}

.pane-bottom-right {
  flex-grow: 0;
  flex-shrink: 0;
  min-height: 0;
  transition: none;
}

.pane {
  overflow: auto;
  min-width: 0;
  transition: none;
}

.resize-handle {
  flex: 0 0 auto;
  position: relative;
  z-index: 1;
}

.resize-handle-vertical {
  width: 6px;
  margin: 0 -3px;
  cursor: col-resize;
  touch-action: none;
}

.resize-handle-horizontal {
  height: 6px;
  margin: -3px 0;
  cursor: row-resize;
  touch-action: none;
}

.resize-handle:hover,
.resize-handle:active {
  background: var(--secondary);
  opacity: 0.5;
}
</style>
