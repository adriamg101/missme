<!-- src/components/ui/SelectField.vue -->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

export interface SelectFieldOption {
  value: string;
  label: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: SelectFieldOption[];
    label?: string;
    ariaLabel?: string;
    size?: "sm" | "md" | "lg";
  }>(),
  { size: "md" },
);
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const isOpen = ref(false);
const rootEl = ref<HTMLElement | null>(null);

const selectedOption = computed(() => props.options.find((option) => option.value === props.modelValue) ?? null);

function toggle() {
  isOpen.value = !isOpen.value;
}

function close() {
  isOpen.value = false;
}

function choose(value: string) {
  emit("update:modelValue", value);
  close();
}

function onDocumentClick(event: MouseEvent) {
  if (!rootEl.value) return;
  if (!rootEl.value.contains(event.target as Node)) close();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") close();
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
  document.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick);
  document.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <div class="select-field-wrapper" ref="rootEl">
    <span v-if="label" class="select-field-label">{{ label }}</span>
    <div class="select-field-root">
      <button
        type="button"
        class="select-field"
        :class="`size-${size}`"
        :aria-label="ariaLabel"
        aria-haspopup="listbox"
        :aria-expanded="isOpen"
        @click="toggle"
      >
        <span class="select-field-value">{{ selectedOption?.label ?? "" }}</span>
        <svg class="select-field-chevron" viewBox="0 0 12 12" width="10" height="10">
          <path fill="currentColor" d="M2.5 4.5l3.5 3.5 3.5-3.5z" />
        </svg>
      </button>

      <ul v-if="isOpen" class="select-field-listbox" role="listbox">
        <li v-for="option in options" :key="option.value">
          <button
            type="button"
            class="select-field-option"
            :class="{ 'is-selected': option.value === modelValue }"
            role="option"
            :aria-selected="option.value === modelValue"
            @click="choose(option.value)"
          >
            {{ option.label }}
          </button>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.select-field-wrapper {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.select-field-label {
  font-size: 0.875rem;
  color: var(--text);
}

.select-field-root {
  position: relative;
}

.select-field {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.4rem;
  background-color: var(--background);
  color: var(--text);
  border: 1px solid var(--secondary);
  border-radius: 6px;
  font-family: inherit;
  cursor: pointer;
}

.select-field:hover {
  border-color: var(--accent);
}

.select-field:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.select-field-value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.select-field-chevron {
  flex: 0 0 auto;
  opacity: 0.6;
}

.select-field-listbox {
  position: absolute;
  top: calc(100% + 0.3rem);
  left: 0;
  right: 0;
  min-width: max-content;
  margin: 0;
  padding: 0.3rem;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  border: 1px solid var(--secondary);
  border-radius: 8px;
  background: var(--background);
  box-shadow: var(--shadow);
  z-index: 15;
  max-height: 14rem;
  overflow-y: auto;
}

.select-field-option {
  width: 100%;
  padding: 0.35rem 0.5rem;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text);
  font-family: inherit;
  font-size: inherit;
  text-align: left;
  cursor: pointer;
}

.select-field-option:hover,
.select-field-option:focus-visible {
  background: var(--secondary);
  color: var(--background);
  outline: none;
}

.select-field-option.is-selected {
  background: var(--accent);
  color: var(--background);
}

.select-field.size-sm {
  min-height: var(--control-height-sm);
  padding: var(--control-padding-sm);
  font-size: var(--control-font-sm);
}

.select-field.size-md {
  min-height: var(--control-height-md);
  padding: var(--control-padding-md);
  font-size: var(--control-font-md);
}

.select-field.size-lg {
  min-height: var(--control-height-lg);
  padding: var(--control-padding-lg);
  font-size: var(--control-font-lg);
}
</style>
