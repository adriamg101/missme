<!-- src/components/ui/TextField.vue -->
<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue: string;
    type?: string;
    placeholder?: string;
    label?: string;
    ariaLabel?: string;
    size?: "sm" | "md" | "lg";
  }>(),
  { type: "text", size: "md" },
);
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <div class="text-field-wrapper">
    <span v-if="label" class="text-field-label">{{ label }}</span>
    <input
      class="text-field"
      :class="`size-${size}`"
      :type="type"
      :placeholder="placeholder"
      :aria-label="ariaLabel"
      :value="modelValue"
      @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
  </div>
</template>

<style scoped>
.text-field-wrapper {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.text-field-label {
  font-size: 0.875rem;
  color: var(--text);
}

.text-field {
  border: 1px solid var(--secondary);
  border-radius: 6px;
  background: var(--background);
  color: var(--text);
  font-family: inherit;
}

.text-field:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.text-field.size-sm {
  min-height: var(--control-height-sm);
  padding: var(--control-padding-sm);
  font-size: var(--control-font-sm);
}

.text-field.size-md {
  min-height: var(--control-height-md);
  padding: var(--control-padding-md);
  font-size: var(--control-font-md);
}

.text-field.size-lg {
  min-height: var(--control-height-lg);
  padding: var(--control-padding-lg);
  font-size: var(--control-font-lg);
}
</style>
