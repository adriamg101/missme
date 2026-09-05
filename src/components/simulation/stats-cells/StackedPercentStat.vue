<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  label: string;
  segments: { label: string; value: number; color?: string }[];
  // Which labels are unchecked, kept by the caller so it survives this component being unmounted and remounted.
  excluded?: string[];
}>();
const emit = defineEmits<{ "update:excluded": [string[]] }>();

const SEGMENT_COLORS = ["#e07a5f", "#3d5a80", "#81b29a", "#f2cc8f", "#9b5de5", "#00b4d8", "#f15bb5", "#ffb703"];

const excludedSet = computed(() => new Set(props.excluded ?? []));

function toggle(label: string) {
  const next = new Set(excludedSet.value);
  if (next.has(label)) next.delete(label);
  else next.add(label);
  emit("update:excluded", [...next]);
}

const coloredSegments = computed(() =>
  props.segments.map((segment, index) => ({
    ...segment,
    value: Math.max(0, Number.isFinite(segment.value) ? segment.value : 0),
    color: segment.color ?? SEGMENT_COLORS[index % SEGMENT_COLORS.length],
  })),
);

// The percentage shown is always relative to the checked levels, not to the whole total.
const checkedTotal = computed(() =>
  coloredSegments.value.reduce((sum, segment) => (excludedSet.value.has(segment.label) ? sum : sum + segment.value), 0),
);

const displaySegments = computed(() =>
  coloredSegments.value.map((segment) => {
    const checked = !excludedSet.value.has(segment.label);
    const fraction = checked && checkedTotal.value > 0 ? segment.value / checkedTotal.value : 0;
    return { ...segment, checked, fraction };
  }),
);
</script>

<template>
  <div class="stat-cell">
    <span class="stat-cell-label">{{ label }}</span>
    <div class="stat-cell-stacked-bar">
      <div
        v-for="segment in displaySegments"
        :key="segment.label"
        class="stat-cell-stacked-segment"
        :style="{ width: `${segment.fraction * 100}%`, background: segment.color }"
      ></div>
    </div>
    <ul class="stat-cell-legend">
      <li v-for="segment in displaySegments" :key="segment.label">
        <input
          type="checkbox"
          class="stat-cell-legend-checkbox"
          :checked="segment.checked"
          :aria-label="`Include ${segment.label} in the percentages`"
          @change="toggle(segment.label)"
        />
        <span class="stat-cell-legend-swatch" :style="{ background: segment.color }"></span>
        <span class="stat-cell-legend-label">{{ segment.label }}</span>
        <span class="stat-cell-legend-percent">{{ segment.checked ? `${(segment.fraction * 100).toFixed(1)}%` : "—" }}</span>
      </li>
    </ul>
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

.stat-cell-stacked-bar {
  display: flex;
  height: 0.6rem;
  border-radius: 999px;
  overflow: hidden;
  background: var(--secondary);
  transition: background-color var(--theme-transition-duration) ease;
}

.stat-cell-legend {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  transition: none;
}

.stat-cell-legend li {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.7rem;
  opacity: 0.85;
  transition: none;
}

.stat-cell-legend-checkbox {
  flex: 0 0 auto;
  accent-color: var(--accent);
}

.stat-cell-legend-swatch {
  width: 0.6rem;
  height: 0.6rem;
  border-radius: 2px;
  flex: 0 0 auto;
}

.stat-cell-legend-label {
  flex: 1 1 auto;
  transition: none;
}

.stat-cell-legend-percent {
  flex: 0 0 auto;
  transition: none;
}
</style>
