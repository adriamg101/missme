<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ label: string; value: number }>();

const clamped = computed(() => Math.max(0, Math.min(1, Number.isFinite(props.value) ? props.value : 0)));
const percentLabel = computed(() => `${(clamped.value * 100).toFixed(1)}%`);
</script>

<template>
  <div class="stat-cell">
    <span class="stat-cell-label">{{ label }}</span>
    <div class="stat-cell-bar-row">
      <div class="stat-cell-bar-track">
        <div class="stat-cell-bar-fill" :style="{ width: `${clamped * 100}%` }"></div>
      </div>
      <span class="stat-cell-bar-percent">{{ percentLabel }}</span>
    </div>
  </div>
</template>

<style scoped>
.stat-cell {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  padding: 0.75rem 1rem;
  border-radius: 8px;
  border: 1px solid var(--secondary);
  background: var(--background);
  transition: border-color var(--theme-transition-duration) ease, background-color var(--theme-transition-duration) ease;
}

.stat-cell-label {
  font-size: 0.75rem;
  opacity: 0.75;
  transition: none;
}

.stat-cell-bar-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  transition: none;
}

.stat-cell-bar-track {
  flex: 1 1 auto;
  height: 0.6rem;
  border-radius: 999px;
  background: var(--secondary);
  overflow: hidden;
  transition: background-color var(--theme-transition-duration) ease;
}

.stat-cell-bar-fill {
  height: 100%;
  background: var(--accent);
  transition: background-color var(--theme-transition-duration) ease;
}

.stat-cell-bar-percent {
  flex: 0 0 auto;
  font-size: 0.7rem;
  opacity: 0.75;
  transition: none;
}
</style>
