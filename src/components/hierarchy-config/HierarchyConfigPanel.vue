<script setup lang="ts">
import { onMounted, ref } from "vue";
import GlobalConfigBar from "./GlobalConfigBar.vue";
import LevelTabs from "./LevelTabs.vue";
import IconButton from "../ui/IconButton.vue";
import ConfirmModal from "../ui/ConfirmModal.vue";
import SaveAsModal from "../ui/SaveAsModal.vue";
import SelectField from "../ui/SelectField.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { listSavedConfigNames, loadSavedConfig, renameConfig, saveConfig } from "../../lib/hierarchyConfigFiles";

const store = useHierarchyConfigStore();

type PendingAction = { kind: "new" } | { kind: "open"; name: string };

const showConfirmModal = ref(false);
const showSaveAsModal = ref(false);
const showRenameModal = ref(false);
const pendingAction = ref<PendingAction | null>(null);

const savedNames = ref<string[]>([]);
const selectedName = ref<string>("");

async function refreshSavedNames() {
  try {
    savedNames.value = await listSavedConfigNames();
  } catch (error) {
    console.error("Failed to list saved hierarchy configs:", error);
  }
}

onMounted(refreshSavedNames);

function hasUnsavedContent(): boolean {
  return store.hierarchy.length > 0;
}

async function performAction(action: PendingAction) {
  if (action.kind === "new") {
    store.resetToDefaults();
    selectedName.value = "";
    return;
  }

  const state = await loadSavedConfig(action.name);
  store.$patch(state as object);
  selectedName.value = action.name;
}

function requestAction(action: PendingAction) {
  if (!hasUnsavedContent()) {
    performAction(action);
    return;
  }

  pendingAction.value = action;
  showConfirmModal.value = true;
}

function closeModal() {
  showConfirmModal.value = false;
  pendingAction.value = null;
}

function handleNewFileClick() {
  requestAction({ kind: "new" });
}

function handleSelectConfig(name: string) {
  if (!name || name === selectedName.value) return;
  requestAction({ kind: "open", name });
}

async function handleOverwrite() {
  const action = pendingAction.value;
  closeModal();
  if (action) await performAction(action);
}

async function handleSaveThenLoad() {
  await doSave(selectedName.value || undefined);

  const action = pendingAction.value;
  closeModal();
  if (action) await performAction(action);
}

function handleCancel() {
  closeModal();
}

async function doSave(name: string | undefined) {
  if (!name) return;
  await saveConfig(name, store.$state);
  selectedName.value = name;
  await refreshSavedNames();
}

function handleSaveClick() {
  if (selectedName.value) {
    doSave(selectedName.value);
    return;
  }
  showSaveAsModal.value = true;
}

async function handleSaveAsConfirm(name: string) {
  showSaveAsModal.value = false;
  await doSave(name);
}

function handleSaveAsCancel() {
  showSaveAsModal.value = false;
}

function handleRenameClick() {
  if (!selectedName.value) return;
  showRenameModal.value = true;
}

async function handleRenameConfirm(newName: string) {
  showRenameModal.value = false;
  if (newName === selectedName.value) return;
  await renameConfig(selectedName.value, newName);
  selectedName.value = newName;
  await refreshSavedNames();
}

function handleRenameCancel() {
  showRenameModal.value = false;
}
</script>

<template>
  <div class="hierarchy-config-panel">
    <div class="hierarchy-config-panel-header">
      <IconButton class="hierarchy-config-panel-new-button" size="sm" ariaLabel="New configuration" @click="handleNewFileClick">
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path
            fill="currentColor"
            d="M4 1.5A1.5 1.5 0 0 0 2.5 3v10A1.5 1.5 0 0 0 4 14.5h8a1.5 1.5 0 0 0 1.5-1.5V6a1 1 0 0 0-.3-.7l-3.5-3.5A1 1 0 0 0 9 1.5H4Zm0 1.5h4.5v2.25A1.25 1.25 0 0 0 9.75 6.5H12V13H4V3Z"
          />
          <path
            fill="currentColor"
            d="M7.4 8.4a.6.6 0 1 1 1.2 0v1h1a.6.6 0 1 1 0 1.2h-1v1a.6.6 0 1 1-1.2 0v-1h-1a.6.6 0 1 1 0-1.2h1v-1Z"
          />
        </svg>
      </IconButton>

      <IconButton size="sm" ariaLabel="Save configuration" @click="handleSaveClick">
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path
            fill="currentColor"
            d="M3 1.5A1.5 1.5 0 0 0 1.5 3v10A1.5 1.5 0 0 0 3 14.5h10a1.5 1.5 0 0 0 1.5-1.5V5.12a1.5 1.5 0 0 0-.44-1.06l-2.12-2.12a1.5 1.5 0 0 0-1.06-.44H3Zm1 1h6v3H4v-3Zm5.5 12h-3v-4h3v4Z"
          />
        </svg>
      </IconButton>

      <IconButton size="sm" ariaLabel="Rename configuration" :disabled="!selectedName" @click="handleRenameClick">
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path
            fill="currentColor"
            d="M11.53 1.97a1.5 1.5 0 0 1 2.12 0l.38.38a1.5 1.5 0 0 1 0 2.12l-7.5 7.5a1 1 0 0 1-.39.25l-3 1a1 1 0 0 1-1.27-1.27l1-3a1 1 0 0 1 .25-.39l7.5-7.5-.09-.09Z"
          />
        </svg>
      </IconButton>

      <SelectField
        class="hierarchy-config-panel-select"
        size="sm"
        aria-label="Saved configurations"
        :model-value="selectedName"
        :options="savedNames.map((name) => ({ value: name, label: name }))"
        @update:model-value="handleSelectConfig"
      />
    </div>

    <div class="hierarchy-config-panel-body">
      <GlobalConfigBar />
      <div class="hierarchy-config-panel-divider"></div>
      <LevelTabs />
    </div>

    <ConfirmModal
      v-if="showConfirmModal"
      message="There is an unsaved hierarchy in the configuration. What do you want to do?"
      @save="handleSaveThenLoad"
      @discard="handleOverwrite"
      @cancel="handleCancel"
    />

    <SaveAsModal v-if="showSaveAsModal" @save="handleSaveAsConfirm" @cancel="handleSaveAsCancel" />

    <SaveAsModal
      v-if="showRenameModal"
      :suggested-name="selectedName"
      prompt-text="Rename this configuration to:"
      confirm-label="Rename"
      @save="handleRenameConfirm"
      @cancel="handleRenameCancel"
    />
  </div>
</template>

<style scoped>
.hierarchy-config-panel {
  position: relative;
  width: calc(100% - 0.5rem);
  height: calc(100% - 0.5rem);
  margin: 0.25rem;
  box-sizing: border-box;
  border-radius: 12px;
  border: 1px solid var(--secondary);
  background: var(--background);
  box-shadow: var(--shadow);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.hierarchy-config-panel-header {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.4rem 0.6rem;
  border-bottom: 1px solid var(--secondary);
}

.hierarchy-config-panel-new-button {
  margin-right: 0;
}

.hierarchy-config-panel-select {
  flex: 1 1 auto;
  min-width: 0;
}

.hierarchy-config-panel-body {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-height: 0;
  padding: 1rem 0;
  overflow-y: auto;
}

.hierarchy-config-panel-divider {
  height: 1px;
  margin: 0 0.875rem;
  background: var(--secondary);
  opacity: 0.4;
}
</style>
