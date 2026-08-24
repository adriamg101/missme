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

const speedText = computed({
  get: () => String(store.speedMs),
  set: (value: string) => {
    const ms = Number(value);
    if (Number.isFinite(ms)) store.setSpeed(ms);
  },
});
</script>

<template>
  <div class="simulation-toolbar">
    <IconButton size="sm" aria-label="Play/Pause" @click="togglePlayPause">
      <svg v-if="!store.isRunning" viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M4 2.5v11l10-5.5-10-5.5Z" />
      </svg>
      <svg v-else viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M4 2.5h3v11H4v-11Zm5 0h3v11H9v-11Z" />
      </svg>
    </IconButton>

    <IconButton size="sm" aria-label="Stop" @click="store.stop">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M3.5 3.5h9v9h-9v-9Z" />
      </svg>
    </IconButton>

    <IconButton size="sm" aria-label="Step action" :disabled="store.isRunning" @click="store.stepAction">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M4 2.5v11l7-5.5-7-5.5Zm8 0h1.5v11H12v-11Z" />
      </svg>
    </IconButton>

    <IconButton size="sm" aria-label="Step instruction" :disabled="store.isRunning" @click="store.stepInstruction">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <path fill="currentColor" d="M2 2.5v11l5-5.5-5-5.5Zm6 0v11l5-5.5-5-5.5Z" />
      </svg>
    </IconButton>

    <input
      class="simulation-toolbar-speed"
      type="range"
      min="10"
      max="1000"
      step="10"
      aria-label="Delay (ms)"
      :value="speedText"
      @input="speedText = ($event.target as HTMLInputElement).value"
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
