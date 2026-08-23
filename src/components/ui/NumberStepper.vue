<!-- src/components/ui/NumberStepper.vue -->
<script setup lang="ts">
import { ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    modelValue: number;
    min?: number;
    max?: number;
    step?: number;
    label?: string;
    ariaLabel?: string;
    size?: "sm" | "md" | "lg";
  }>(),
  { step: 1, size: "md" },
);
const emit = defineEmits<{ "update:modelValue": [value: number] }>();

const draft = ref(String(props.modelValue));
watch(
  () => props.modelValue,
  (value) => {
    draft.value = String(value);
  },
);

function clamp(value: number): number {
  let result = value;
  if (props.min !== undefined && result < props.min) result = props.min;
  if (props.max !== undefined && result > props.max) result = props.max;
  return result;
}

function increment() {
  const next = clamp(props.modelValue + props.step);
  if (next === props.modelValue) return;
  emit("update:modelValue", next);
}

function decrement() {
  const next = clamp(props.modelValue - props.step);
  if (next === props.modelValue) return;
  emit("update:modelValue", next);
}

function onBlur() {
  const parsed = Number(draft.value);
  const rounded = clamp(Number.isFinite(parsed) ? parsed : props.modelValue);
  draft.value = String(rounded);
  if (rounded !== props.modelValue) emit("update:modelValue", rounded);
}
</script>

<template>
  <div class="number-stepper-field">
    <span v-if="label" class="number-stepper-label">{{ label }}</span>
    <div class="number-stepper" :class="`size-${size}`">
      <button type="button" class="number-stepper-button" aria-label="Decrementar" @click="decrement">−</button>
      <input
        type="number"
        role="spinbutton"
        class="number-stepper-input"
        :aria-label="ariaLabel"
        :min="min"
        :max="max"
        v-model="draft"
        @blur="onBlur"
      />
      <button type="button" class="number-stepper-button" aria-label="Incrementar" @click="increment">+</button>
    </div>
  </div>
</template>

<style scoped>
.number-stepper-field {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.number-stepper-label {
  font-size: 0.875rem;
  color: var(--text);
}

.number-stepper {
  display: flex;
  align-items: stretch;
  border: 1px solid var(--secondary);
  border-radius: 6px;
  overflow: hidden;
}

.number-stepper-button {
  min-width: 1.5rem;
  padding: 0 0.6rem;
  border: none;
  background: var(--background);
  color: var(--text);
  font: inherit;
  font-size: 1rem;
  line-height: 1;
  cursor: pointer;
}

.number-stepper-button:hover {
  background: var(--secondary);
}

.number-stepper-button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.number-stepper-input {
  width: 100%;
  min-width: 0;
  border: none;
  border-left: 1px solid var(--secondary);
  border-right: 1px solid var(--secondary);
  background: var(--background);
  color: var(--text);
  font: inherit;
  text-align: center;
}

.number-stepper-input:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.number-stepper.size-sm .number-stepper-input {
  min-height: var(--control-height-sm);
  padding: var(--control-padding-sm);
  font-size: var(--control-font-sm);
}

.number-stepper.size-md .number-stepper-input {
  min-height: var(--control-height-md);
  padding: var(--control-padding-md);
  font-size: var(--control-font-md);
}

.number-stepper.size-lg .number-stepper-input {
  min-height: var(--control-height-lg);
  padding: var(--control-padding-lg);
  font-size: var(--control-font-lg);
}
</style>
