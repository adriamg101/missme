<!-- src/components/ui/SaveAsModal.vue -->
<script setup lang="ts">
import { ref } from "vue";

const props = withDefaults(
  defineProps<{ suggestedName?: string; promptText?: string; confirmLabel?: string }>(),
  { promptText: "Save this hierarchy configuration as:", confirmLabel: "Save" },
);
const emit = defineEmits<{ save: [name: string]; cancel: [] }>();

const name = ref(props.suggestedName ?? "");

function onSave() {
  const trimmed = name.value.trim();
  if (!trimmed) return;
  emit("save", trimmed);
}
</script>

<template>
  <div class="confirm-modal-overlay">
    <div class="confirm-modal">
      <p class="confirm-modal-text">{{ promptText }}</p>
      <input
        type="text"
        class="save-as-modal-input"
        aria-label="Configuration name"
        v-model="name"
        @keydown.enter="onSave"
      />
      <div class="confirm-modal-actions">
        <button type="button" class="confirm-modal-button" :disabled="!name.trim()" @click="onSave">{{ confirmLabel }}</button>
        <button type="button" class="confirm-modal-button confirm-modal-button-cancel" @click="emit('cancel')">
          Cancel
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.confirm-modal-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
}

.confirm-modal {
  width: min(320px, 80%);
  padding: 1rem;
  border-radius: 10px;
  border: 1px solid var(--secondary);
  background: var(--background);
  color: var(--text);
}

.confirm-modal-text {
  margin: 0 0 0.75rem;
  font-size: 0.85rem;
}

.save-as-modal-input {
  width: 100%;
  box-sizing: border-box;
  padding: 0.4rem 0.6rem;
  margin-bottom: 0.75rem;
  border-radius: 6px;
  border: 1px solid var(--secondary);
  background: var(--background);
  color: var(--text);
  font-family: inherit;
  font-size: 0.85rem;
}

.confirm-modal-actions {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.confirm-modal-button {
  padding: 0.4rem 0.6rem;
  border-radius: 6px;
  border: 1px solid var(--secondary);
  background: transparent;
  color: var(--text);
  font-family: inherit;
  font-size: 0.8rem;
  cursor: pointer;
}

.confirm-modal-button:hover {
  background: var(--secondary);
  opacity: 0.8;
}

.confirm-modal-button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.confirm-modal-button-cancel {
  border-color: transparent;
  opacity: 0.7;
}
</style>
