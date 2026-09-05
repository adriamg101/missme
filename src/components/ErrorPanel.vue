<script setup lang="ts">
import { ref } from "vue";

defineProps<{
  count: number;
  output: string;
}>();

const expanded = ref(false);

function toggle() {
  expanded.value = !expanded.value;
}
</script>

<template>
  <div v-if="count > 0" class="error-panel">
    <button type="button" class="error-panel-header" @click="toggle">
      <span>{{ count }} {{ count === 1 ? "error" : "errores" }}</span>
      <svg
        class="error-panel-chevron"
        :class="{ 'error-panel-chevron-expanded': expanded }"
        viewBox="0 0 12 12"
        width="10"
        height="10"
      >
        <path d="M2 8l4-4 4 4" fill="none" stroke="currentColor" stroke-width="1.4" />
      </svg>
    </button>

    <div v-if="expanded" class="error-panel-output">{{ output }}</div>
  </div>
</template>

<style scoped>
.error-panel {
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
  max-height: 75%;
  border-top: 1px solid var(--secondary);
  transition: border-color var(--theme-transition-duration) ease;
}

.error-panel-header {
  flex: 0 0 auto;
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.35rem 0.6rem;
  border: none;
  background: transparent;
  color: var(--text);
  font-family: inherit;
  font-size: 0.75rem;
  cursor: pointer;
  transition: color var(--theme-transition-duration) ease;
}

.error-panel-header:hover {
  background: var(--secondary);
  opacity: 0.8;
}

.error-panel-chevron {
  transition: transform 0.2s ease, stroke var(--theme-transition-duration) ease;
}

.error-panel-chevron-expanded {
  transform: rotate(180deg);
}

.error-panel-output {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  padding: 0.5rem 0.6rem;
  margin: 0;
  white-space: pre-wrap;
  font-family: 'IBM Plex Mono', monospace;
  font-size: 0.75rem;
  color: var(--text);
  border-top: 1px solid var(--secondary);
  transition: color var(--theme-transition-duration) ease, border-color var(--theme-transition-duration) ease;
}
</style>
