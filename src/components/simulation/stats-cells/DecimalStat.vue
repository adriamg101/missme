<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ label: string; value: number; decimals: number }>();

const formatted = computed(() => (Number.isFinite(props.value) ? props.value.toFixed(props.decimals) : "0"));
</script>

<template>
  <div class="stat-cell">
    <span class="stat-cell-label">{{ label }}</span>
    <span class="stat-cell-value">{{ formatted }}</span>
  </div>
</template>

<style scoped>
.stat-cell {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
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

.stat-cell-value {
  font-size: 1.4rem;
  font-weight: 600;
  transition: none;
}
</style>
