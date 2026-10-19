<script setup lang="ts">
import { ref, watch } from "vue";
import { sizeLabel, parseSizeInput, roundToPowerOfTwo } from "../../lib/hierarchy/derive";

const props = withDefaults(
  defineProps<{
    modelValue: number;
    min?: number;
    max?: number;
    label?: string;
    ariaLabel?: string;
    size?: "sm" | "md" | "lg";
  }>(),
  { min: 1, size: "md" },
);
const emit = defineEmits<{ "update:modelValue": [value: number] }>();

function display(value: number): string {
  return sizeLabel(Math.log2(value));
}

const draft = ref(display(props.modelValue));
const isInvalid = ref(false);
let invalidTimeout: ReturnType<typeof setTimeout> | null = null;

watch(
  () => props.modelValue,
  (value) => {
    draft.value = display(value);
  },
);

function clamp(value: number): number {
  let result = value;
  if (result < props.min) result = props.min;
  if (props.max !== undefined && result > props.max) result = props.max;
  return result;
}

function increment() {
  const next = clamp(props.modelValue * 2);
  if (next === props.modelValue) return;
  emit("update:modelValue", next);
}

function decrement() {
  const next = clamp(Math.max(props.min, props.modelValue / 2));
  if (next === props.modelValue) return;
  emit("update:modelValue", next);
}

function flashInvalid() {
  isInvalid.value = true;
  if (invalidTimeout) clearTimeout(invalidTimeout);
  invalidTimeout = setTimeout(() => {
    isInvalid.value = false;
  }, 400);
}

function onBlur() {
  const parsed = parseSizeInput(draft.value);
  if (parsed === null) {
    flashInvalid();
    draft.value = display(props.modelValue);
    return;
  }
  const rounded = clamp(roundToPowerOfTwo(parsed));
  draft.value = display(rounded);
  if (rounded !== props.modelValue) emit("update:modelValue", rounded);
}
</script>

<template>
  <div class="power-of-two-stepper-field">
    <span v-if="label" class="power-of-two-stepper-label">{{ label }}</span>
    <div class="power-of-two-stepper" :class="`size-${size}`">
      <button type="button" class="power-of-two-stepper-button" aria-label="Decrement" @click="decrement">−</button>
      <input
        type="text"
        role="spinbutton"
        class="power-of-two-stepper-input"
        :class="{ 'power-of-two-stepper-input-invalid': isInvalid }"
        :aria-label="ariaLabel"
        v-model="draft"
        @blur="onBlur"
      />
      <button type="button" class="power-of-two-stepper-button" aria-label="Increment" @click="increment">+</button>
    </div>
  </div>
</template>

<style scoped>
.power-of-two-stepper-field {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.power-of-two-stepper-label {
  font-size: 0.875rem;
  color: var(--text);
  transition: color var(--theme-transition-duration) ease;
}

.power-of-two-stepper {
  display: flex;
  align-items: stretch;
  border: 1px solid var(--secondary);
  border-radius: 6px;
  overflow: hidden;
  transition: border-color var(--theme-transition-duration) ease;
}

.power-of-two-stepper-button {
  min-width: 1.5rem;
  padding: 0 0.6rem;
  border: none;
  background: var(--background);
  color: var(--text);
  font: inherit;
  font-size: 1rem;
  line-height: 1;
  cursor: pointer;
  transition: background-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}

.power-of-two-stepper-button:hover {
  background: var(--secondary);
}

.power-of-two-stepper-button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.power-of-two-stepper-input {
  width: 100%;
  min-width: 0;
  border: none;
  border-left: 1px solid var(--secondary);
  border-right: 1px solid var(--secondary);
  background: var(--background);
  color: var(--text);
  font: inherit;
  text-align: center;
  transition: border-color var(--theme-transition-duration) ease, background-color var(--theme-transition-duration) ease, color var(--theme-transition-duration) ease;
}

.power-of-two-stepper-input:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.power-of-two-stepper-input-invalid {
  border-color: var(--diagram-miss-color);
  background: color-mix(in srgb, var(--diagram-miss-color), transparent 85%);
}

.power-of-two-stepper.size-sm .power-of-two-stepper-input {
  min-height: var(--control-height-sm);
  padding: var(--control-padding-sm);
  font-size: var(--control-font-sm);
}

.power-of-two-stepper.size-md .power-of-two-stepper-input {
  min-height: var(--control-height-md);
  padding: var(--control-padding-md);
  font-size: var(--control-font-md);
}

.power-of-two-stepper.size-lg .power-of-two-stepper-input {
  min-height: var(--control-height-lg);
  padding: var(--control-padding-lg);
  font-size: var(--control-font-lg);
}
</style>
