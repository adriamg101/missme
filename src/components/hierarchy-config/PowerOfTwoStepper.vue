<script setup lang="ts">
import { ref, watch } from "vue";

/**
 * Numeric stepper for powers of two.
 * INVARIANT: min and max must themselves be powers of two; the component does not enforce this at runtime.
 */
const props = withDefaults(defineProps<{ modelValue: number; min?: number; max?: number }>(), {
  min: 1,
});
const emit = defineEmits<{ "update:modelValue": [value: number] }>();

const draft = ref(String(props.modelValue));
watch(
  () => props.modelValue,
  (value) => {
    draft.value = String(value);
  },
);

function nearestPowerOfTwo(value: number): number {
  if (value <= props.min) return props.min;
  return 2 ** Math.round(Math.log2(value));
}

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

function onBlur() {
  const parsed = Number(draft.value);
  const rounded = clamp(Number.isFinite(parsed) && parsed > 0 ? nearestPowerOfTwo(parsed) : props.modelValue);
  draft.value = String(rounded);
  if (rounded !== props.modelValue) emit("update:modelValue", rounded);
}
</script>

<template>
  <div class="power-of-two-stepper">
    <button type="button" aria-label="Decrementar" @click="decrement">−</button>
    <input type="number" role="spinbutton" v-model="draft" @blur="onBlur" />
    <button type="button" aria-label="Incrementar" @click="increment">+</button>
  </div>
</template>
