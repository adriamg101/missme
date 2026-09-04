<!-- src/components/ui/ConfirmModal.vue -->
<script setup lang="ts">
defineProps<{ message: string }>();
const emit = defineEmits<{ save: []; discard: []; cancel: [] }>();
</script>

<template>
  <div class="confirm-modal-overlay">
    <div class="confirm-modal">
      <p class="confirm-modal-text">{{ message }}</p>
      <div class="confirm-modal-actions">
        <button type="button" class="confirm-modal-button" @click="emit('save')">Save</button>
        <button type="button" class="confirm-modal-button" @click="emit('discard')">Overwrite without saving</button>
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

.confirm-modal-button-cancel {
  border-color: transparent;
  opacity: 0.7;
}
</style>
