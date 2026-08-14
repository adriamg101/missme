<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import ErrorPanel from "./ErrorPanel.vue";

// TODO: in the future this list should come from the backend
const languages = ["RISC-V", "C"];

const selectedLanguage = ref(languages[0]);
const code = ref("");

const textareaRef = ref<HTMLTextAreaElement | null>(null);
const gutterRef = ref<HTMLDivElement | null>(null);

const lineCount = computed(() => code.value.split("\n").length);

function syncScroll() {
  if (textareaRef.value && gutterRef.value) {
    gutterRef.value.scrollTop = textareaRef.value.scrollTop;
  }
}

const breakpoints = ref(new Set<number>());

function toggleBreakpoint(line: number) {
  if (breakpoints.value.has(line)) {
    breakpoints.value.delete(line);
  } else {
    breakpoints.value.add(line);
  }
}

// TODO: wire to real compiler output once the backend is connected
const errorCount = ref(0);
const errorOutput = ref("");

const isDragOver = ref(false);

type PendingSource = { kind: "open" } | { kind: "drop"; text: string };

const showConfirmModal = ref(false);
const pendingSource = ref<PendingSource | null>(null);

function loadFileContent(text: string) {
  code.value = text;
  breakpoints.value.clear();
  errorCount.value = 0;
  errorOutput.value = "";
}

async function performLoad(source: PendingSource) {
  if (source.kind === "drop") {
    loadFileContent(source.text);
    return;
  }

  const path = await open({ multiple: false });
  if (!path || Array.isArray(path)) return;

  const text = await readTextFile(path);
  loadFileContent(text);
}

function requestLoad(source: PendingSource) {
  if (code.value.trim() === "") {
    performLoad(source);
    return;
  }

  pendingSource.value = source;
  showConfirmModal.value = true;
}

function closeModal() {
  showConfirmModal.value = false;
  pendingSource.value = null;
}

async function handleOpenFileClick() {
  requestLoad({ kind: "open" });
}

async function handleOverwrite() {
  const source = pendingSource.value;
  closeModal();
  if (source) await performLoad(source);
}

async function handleSaveThenLoad() {
  const path = await save();
  if (!path) return; // user cancelled the save dialog, abort the whole load

  await writeTextFile(path, code.value);

  const source = pendingSource.value;
  closeModal();
  if (source) await performLoad(source);
}

function handleCancel() {
  closeModal();
}

const editorBodyRef = ref<HTMLDivElement | null>(null);
let unlistenDragDrop: (() => void) | null = null;

function isPointInEditorBody(physicalX: number, physicalY: number): boolean {
  if (!editorBodyRef.value) return false;
  const scale = window.devicePixelRatio || 1;
  const x = physicalX / scale;
  const y = physicalY / scale;
  const rect = editorBodyRef.value.getBoundingClientRect();
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

async function handleDroppedPath(path: string) {
  const text = await readTextFile(path);
  requestLoad({ kind: "drop", text });
}

onMounted(async () => {
  // Tauri intercepts OS-level file drag & drop before it reaches the DOM
  // (dataTransfer.files stays empty for files dragged from a file manager),
  // so this uses Tauri's own drag & drop event instead of native HTML5 drop events.
  unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "drop") {
      isDragOver.value = false;
      const { x, y } = event.payload.position;
      const [firstPath] = event.payload.paths;
      if (firstPath && isPointInEditorBody(x, y)) {
        handleDroppedPath(firstPath);
      }
    } else if (event.payload.type === "enter" || event.payload.type === "over") {
      const { x, y } = event.payload.position;
      isDragOver.value = isPointInEditorBody(x, y);
    } else {
      isDragOver.value = false;
    }
  });
});

onBeforeUnmount(() => {
  unlistenDragDrop?.();
});
</script>

<template>
  <div class="editor">
    <div class="editor-header">
      <button
        type="button"
        class="editor-icon-button"
        aria-label="Abrir fichero"
        @click="handleOpenFileClick"
      >
        <svg viewBox="0 0 16 16" width="14" height="14">
          <path
            fill="currentColor"
            d="M1.5 3A1.5 1.5 0 0 1 3 1.5h3.379a1.5 1.5 0 0 1 1.06.44l1.122 1.12A1.5 1.5 0 0 0 9.62 3.5H13A1.5 1.5 0 0 1 14.5 5v7A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12V3Z"
          />
        </svg>
      </button>

      <select v-model="selectedLanguage" class="editor-language-select">
        <option v-for="lang in languages" :key="lang" :value="lang">{{ lang }}</option>
      </select>
    </div>

    <div
      ref="editorBodyRef"
      class="editor-body"
      :class="{ 'editor-body-drag-over': isDragOver }"
    >
      <div class="editor-gutter" ref="gutterRef">
        <div
          v-for="n in lineCount"
          :key="n"
          class="editor-gutter-line"
          @click="toggleBreakpoint(n)"
        >
          <span v-if="breakpoints.has(n)" class="editor-breakpoint-dot"></span>
          {{ n }}
        </div>
      </div>
      <textarea
        ref="textareaRef"
        v-model="code"
        class="editor-textarea"
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
        wrap="off"
        @scroll="syncScroll"
      ></textarea>
    </div>

    <ErrorPanel :count="errorCount" :output="errorOutput" />

    <div v-if="showConfirmModal" class="editor-modal-overlay">
      <div class="editor-modal">
        <p class="editor-modal-text">Hay código sin guardar en el editor. ¿Qué quieres hacer?</p>
        <div class="editor-modal-actions">
          <button type="button" class="editor-modal-button" @click="handleSaveThenLoad">
            Guardar
          </button>
          <button type="button" class="editor-modal-button" @click="handleOverwrite">
            Sobrescribir sin guardar
          </button>
          <button type="button" class="editor-modal-button editor-modal-button-cancel" @click="handleCancel">
            Cancelar
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.editor {
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

.editor-header {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 0.4rem;
  justify-content: space-between;
  padding: 0.4rem 0.6rem;
  border-bottom: 1px solid var(--secondary);
}

.editor-icon-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text);
  cursor: pointer;
  margin-right: auto;
}

.editor-icon-button:hover {
  background: var(--secondary);
  opacity: 0.8;
}

.editor-language-select {
  appearance: none;
  -webkit-appearance: none;
  -moz-appearance: none;
  background-color: var(--background);
  color: var(--text);
  border: 1px solid var(--secondary);
  border-radius: 6px;
  padding: 0.15rem 1.2rem 0.15rem 0.4rem;
  font-family: inherit;
  font-size: 0.75rem;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12'%3E%3Cpath fill='%23888' d='M2.5 4.5l3.5 3.5 3.5-3.5z'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 0.35rem center;
  background-size: 8px;
}

.editor-language-select option {
  background-color: var(--background);
  color: var(--text);
}

.editor-body {
  flex: 1 1 auto;
  display: flex;
  min-height: 0;
}

.editor-body-drag-over {
  outline: 2px dashed var(--accent);
  outline-offset: -2px;
}

.editor-gutter {
  flex: 0 0 auto;
  padding: 0.5rem 0.5rem 0.5rem 0;
  text-align: right;
  color: var(--secondary);
  user-select: none;
  overflow: hidden;
  font-family: 'IBM Plex Mono', monospace;
  font-size: 0.85rem;
  line-height: 1.5;
}

.editor-gutter-line {
  position: relative;
  padding: 0 0.5rem;
  cursor: pointer;
}

.editor-breakpoint-dot {
  position: absolute;
  left: 2px;
  top: 50%;
  transform: translateY(-50%);
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #e5484d;
}

.editor-textarea {
  flex: 1 1 auto;
  resize: none;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text);
  font-family: 'IBM Plex Mono', monospace;
  font-size: 0.85rem;
  line-height: 1.5;
  padding: 0.5rem;
  white-space: pre;
  overflow: auto;
}

.editor-modal-overlay {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
}

.editor-modal {
  width: min(320px, 80%);
  padding: 1rem;
  border-radius: 10px;
  border: 1px solid var(--secondary);
  background: var(--background);
  color: var(--text);
}

.editor-modal-text {
  margin: 0 0 0.75rem;
  font-size: 0.85rem;
}

.editor-modal-actions {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.editor-modal-button {
  padding: 0.4rem 0.6rem;
  border-radius: 6px;
  border: 1px solid var(--secondary);
  background: transparent;
  color: var(--text);
  font-family: inherit;
  font-size: 0.8rem;
  cursor: pointer;
}

.editor-modal-button:hover {
  background: var(--secondary);
  opacity: 0.8;
}

.editor-modal-button-cancel {
  border-color: transparent;
  opacity: 0.7;
}
</style>
