<!-- src/components/simulation/SimulationToolbar.vue -->
<script setup lang="ts">
import { computed } from "vue";
import IconButton from "../ui/IconButton.vue";
import { useSimulationStore } from "../../stores/simulation";

const store = useSimulationStore();

function togglePlayPause() {
  if (store.isRunning) {
    store.pause();
  } else {
    store.resume();
  }
}

// The slider moves evenly, but its position is turned into a speed on a curve, so fast speeds still feel spread out.
const MIN_MS = 10;
const MAX_MS = 1000;
const SLIDER_MAX = 100;
const LOG_RATIO = Math.log(MAX_MS / MIN_MS);

function msToSlider(ms: number): number {
  const clamped = Math.min(MAX_MS, Math.max(MIN_MS, ms));
  return Math.round(SLIDER_MAX - (SLIDER_MAX * Math.log(clamped / MIN_MS)) / LOG_RATIO);
}

function sliderToMs(position: number): number {
  const clamped = Math.min(SLIDER_MAX, Math.max(0, position));
  return Math.round(MIN_MS * Math.exp(((SLIDER_MAX - clamped) / SLIDER_MAX) * LOG_RATIO));
}

const speedSlider = computed({
  get: () => String(msToSlider(store.speedMs)),
  set: (value: string) => {
    const position = Number(value);
    if (Number.isFinite(position)) store.setSpeed(sliderToMs(position));
  },
});
</script>

<template>
  <div class="simulation-toolbar">
    <IconButton size="sm" ariaLabel="Play/Pause" @click="togglePlayPause">
      <svg v-if="!store.isRunning" viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M4 2.5v11l10-5.5-10-5.5Z" />
      </svg>
      <svg v-else viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M4 2.5h3v11H4v-11Zm5 0h3v11H9v-11Z" />
      </svg>
    </IconButton>

    <IconButton size="sm" ariaLabel="Stop" @click="store.stop">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M3.5 3.5h9v9h-9v-9Z" />
      </svg>
    </IconButton>

    <IconButton size="sm" ariaLabel="Step action" :disabled="store.isRunning" @click="store.stepAction">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M4 2.5v11l7-5.5-7-5.5Zm8 0h1.5v11H12v-11Z" />
      </svg>
    </IconButton>

    <IconButton size="sm" ariaLabel="Step instruction" :disabled="store.isRunning" @click="store.stepInstruction">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M2 2.5v11l5-5.5-5-5.5Zm6 0v11l5-5.5-5-5.5Z" />
      </svg>
    </IconButton>

    <input
      class="simulation-toolbar-speed"
      type="range"
      min="0"
      :max="SLIDER_MAX"
      step="1"
      ariaLabel="Delay (ms)"
      :value="speedSlider"
      @input="speedSlider = ($event.target as HTMLInputElement).value"
    />
  </div>
</template>

<style scoped>
.simulation-toolbar {
  display: flex;
  align-items: center;
  gap: 0.3rem;
}

.simulation-toolbar-speed {
  width: 6rem;
  accent-color: var(--accent);
}
</style>
