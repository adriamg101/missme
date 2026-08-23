<script setup lang="ts">
import { ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import GlobalConfigBar from "./GlobalConfigBar.vue";
import LevelTabs from "./LevelTabs.vue";
import IconButton from "../ui/IconButton.vue";
import ConfirmModal from "../ui/ConfirmModal.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

const store = useHierarchyConfigStore();

type PendingAction = { kind: "new" } | { kind: "open" };

const showConfirmModal = ref(false);
const pendingAction = ref<PendingAction | null>(null);

function hasUnsavedContent(): boolean {
  return store.hierarchy.length > 0;
}

async function performAction(action: PendingAction) {
  if (action.kind === "new") {
    store.resetToDefaults();
    return;
  }

  const path = await open({ multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
  if (!path || Array.isArray(path)) return;

  const text = await readTextFile(path);
  store.$patch(JSON.parse(text));
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

function handleOpenFileClick() {
  requestAction({ kind: "open" });
}

async function handleOverwrite() {
  const action = pendingAction.value;
  closeModal();
  if (action) await performAction(action);
}

async function handleSaveThenLoad() {
  let path = await save({ filters: [{ name: "JSON", extensions: ["json"] }] });
  if (!path) return; // user cancelled the save dialog, abort the whole action

  if (!path.toLowerCase().endsWith(".json")) path += ".json";
  await writeTextFile(path, JSON.stringify(store.$state, null, 2));

  const action = pendingAction.value;
  closeModal();
  if (action) await performAction(action);
}

function handleCancel() {
  closeModal();
}
</script>

<template>
  <div class="hierarchy-config-panel">
    <div class="hierarchy-config-panel-header">
      <IconButton class="hierarchy-config-panel-new-button" size="sm" ariaLabel="Nueva configuración" @click="handleNewFileClick">
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

      <IconButton size="sm" ariaLabel="Abrir configuración" @click="handleOpenFileClick">
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path
            fill="currentColor"
            d="M1.5 3A1.5 1.5 0 0 1 3 1.5h3.379a1.5 1.5 0 0 1 1.06.44l1.122 1.12A1.5 1.5 0 0 0 9.62 3.5H13A1.5 1.5 0 0 1 14.5 5v7A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12V3Z"
          />
        </svg>
      </IconButton>
    </div>

    <div class="hierarchy-config-panel-body">
      <GlobalConfigBar />
      <div class="hierarchy-config-panel-divider"></div>
      <LevelTabs />
    </div>

    <ConfirmModal
      v-if="showConfirmModal"
      message="Hay una jerarquía sin guardar en la configuración. ¿Qué quieres hacer?"
      @save="handleSaveThenLoad"
      @discard="handleOverwrite"
      @cancel="handleCancel"
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
