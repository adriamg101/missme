<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue: string;
    label?: string;
    ariaLabel?: string;
    readonly?: boolean;
    size?: "sm" | "md" | "lg";
    // Every current use case is a 32-bit hex address, so 8 hex digits is a
    // sensible default limit, it stops values bigger than the max 32-bit
    // number from being typed in.
    maxLength?: number;
  }>(),
  { size: "sm", maxLength: 8 },
);
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

function onInput(event: Event) {
  const input = event.target as HTMLInputElement;
  const raw = input.value;
  const clean = raw
    .replace(/[^0-9a-fA-F]/g, "")
    .toUpperCase()
    .slice(0, props.maxLength);

  // The input field only gets updated automatically when the value passed
  // in from outside actually changes. If the cleaned-up value ends up the
  // same as what was already there, that automatic update never happens,
  // so the field would keep showing whatever the user typed. To avoid
  // that, we set the cleaned value on the field directly here.
  if (input.value !== clean) {
    input.value = clean;
  }

  emit("update:modelValue", clean);
}
</script>

<template>
  <div class="hex-field-wrapper">
    <span v-if="label" class="hex-field-label">{{ label }}</span>
    <div class="hex-field-input-row" :class="{ 'hex-field-readonly': readonly }">
      <span class="hex-field-prefix">0x</span>
      <input
        class="hex-field-input"
        :class="`size-${size}`"
        type="text"
        :readonly="readonly"
        :aria-label="ariaLabel"
        :maxlength="maxLength"
        :value="modelValue"
        @input="onInput"
      />
    </div>
  </div>
</template>

<style scoped>
.hex-field-wrapper {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.hex-field-label {
  font-size: 0.875rem;
  color: var(--text);
}

.hex-field-input-row {
  display: flex;
  align-items: center;
  border: 1px solid var(--secondary);
  border-radius: 6px;
  background: var(--background);
  overflow: hidden;
}

.hex-field-input-row:focus-within {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.hex-field-readonly {
  opacity: 0.6;
}

.hex-field-prefix {
  padding-left: 0.4rem;
  color: var(--secondary);
  font-family: 'IBM Plex Mono', monospace;
  user-select: none;
}

.hex-field-input {
  border: none;
  outline: none;
  background: transparent;
  color: var(--text);
  font-family: 'IBM Plex Mono', monospace;
  flex: 1 1 auto;
  min-width: 0;
}

.hex-field-input.size-sm {
  min-height: var(--control-height-sm);
  padding: var(--control-padding-sm);
  font-size: var(--control-font-sm);
}

.hex-field-input.size-md {
  min-height: var(--control-height-md);
  padding: var(--control-padding-md);
  font-size: var(--control-font-md);
}

.hex-field-input.size-lg {
  min-height: var(--control-height-lg);
  padding: var(--control-padding-lg);
  font-size: var(--control-font-lg);
}
</style>
