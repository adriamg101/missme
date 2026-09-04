<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { invoke } from "@tauri-apps/api/core";
import ErrorPanel from "./ErrorPanel.vue";
import SelectField from "./ui/SelectField.vue";
import IconButton from "./ui/IconButton.vue";
import ConfirmModal from "./ui/ConfirmModal.vue";
import { useHierarchyConfigStore } from "../stores/hierarchyConfig";
import CompileConfigForm from "./compile-config/CompileConfigForm.vue";
import { useCompileConfigStore } from "../stores/compileConfig";
import SimulationToolbar from "./simulation/SimulationToolbar.vue";
import { useSimulationStore } from "../stores/simulation";

// TODO: in the future this list should come from the backend instead of being hardcoded here
const languages = ["RISC-V"];
const languageOptions = languages.map((lang) => ({ value: lang, label: lang }));

const selectedLanguage = ref(languages[0]);
const code = ref("");

// Only these languages have a real compile form to show, right now just RISC-V.
const languagesWithCompileConfig = new Set(["RISC-V"]);
const languageHasCompileConfig = computed(() => languagesWithCompileConfig.has(selectedLanguage.value));

type RiscvExtension = "M" | "F" | "D" | "Q";

const hierarchyConfigStore = useHierarchyConfigStore();
const compileConfigStore = useCompileConfigStore();
const simulationStore = useSimulationStore();
const riscvBase = computed({
  get: () => hierarchyConfigStore.isa,
  set: (value) => hierarchyConfigStore.setIsa(value),
});
const riscvExtensions = ref<Set<RiscvExtension>>(new Set());
const isOptionsMenuOpen = ref(false);
const optionsMenuRef = ref<HTMLDivElement | null>(null);

function toggleOptionsMenu() {
  isOptionsMenuOpen.value = !isOptionsMenuOpen.value;
}

function selectBase(base: "RV32I" | "RV64I") {
  riscvBase.value = base;
}

// D extends F in the real spec, so D can never be on without F.
function toggleExtension(extension: RiscvExtension) {
  if (riscvExtensions.value.has(extension)) {
    riscvExtensions.value.delete(extension);
    if (extension === "F") riscvExtensions.value.delete("D");
  } else {
    riscvExtensions.value.add(extension);
    if (extension === "D") riscvExtensions.value.add("F");
  }
}

function handleDocumentClick(event: MouseEvent) {
  if (!optionsMenuRef.value) return;
  if (!optionsMenuRef.value.contains(event.target as Node)) {
    isOptionsMenuOpen.value = false;
  }
}

const textareaRef = ref<HTMLTextAreaElement | null>(null);
const gutterRef = ref<HTMLDivElement | null>(null);

const lineCount = computed(() => code.value.split("\n").length);

const scrollTop = ref(0);

function syncScroll() {
  if (textareaRef.value && gutterRef.value) {
    gutterRef.value.scrollTop = textareaRef.value.scrollTop;
    scrollTop.value = textareaRef.value.scrollTop;
  }
}

const lineHeightPx = ref(20);

// Matches the text area's own top spacing so the highlight lines up with the actual text.
const TEXTAREA_TOP_PADDING_PX = 8;

function measureLineHeight() {
  if (!gutterRef.value) return;
  const firstLine = gutterRef.value.querySelector(".editor-gutter-line");
  if (firstLine) {
    lineHeightPx.value = (firstLine as HTMLElement).getBoundingClientRect().height;
  } else if (lineCount.value > 0) {
    lineHeightPx.value = gutterRef.value.scrollHeight / lineCount.value;
  }
}

const currentLineHighlightStyle = computed(() => {
  if (simulationStore.currentLine === null) return { display: "none" };
  const top =
    (simulationStore.currentLine - 1) * lineHeightPx.value - scrollTop.value + TEXTAREA_TOP_PADDING_PX;
  return {
    position: "absolute" as const,
    left: "0",
    right: "0",
    top: `${top}px`,
    height: `${lineHeightPx.value}px`,
    background: "var(--accent)",
    opacity: "0.15",
    pointerEvents: "none" as const,
  };
});

watch(lineCount, measureLineHeight);

const breakpoints = ref(new Set<number>());

function toggleBreakpoint(line: number) {
  if (breakpoints.value.has(line)) {
    breakpoints.value.delete(line);
  } else {
    breakpoints.value.add(line);
  }
}

const errorCount = ref(0);
const errorOutput = ref("");

const isDragOver = ref(false);

type PendingSource = { kind: "open" } | { kind: "drop"; text: string } | { kind: "new" };

const showConfirmModal = ref(false);
const pendingSource = ref<PendingSource | null>(null);

function loadFileContent(text: string) {
  code.value = text;
  breakpoints.value.clear();
  errorCount.value = 0;
  errorOutput.value = "";
}

async function performLoad(source: PendingSource) {
  if (source.kind === "new") {
    loadFileContent("");
    return;
  }

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

function handleNewFileClick() {
  requestLoad({ kind: "new" });
}

const isCompiling = ref(false);

async function handleStartClick() {
  isCompiling.value = true;
  errorCount.value = 0;
  errorOutput.value = "";

  try {
    const result = await invoke<{
      error_count: number;
      error_output: string;
      config: unknown;
      codegen: { entries: unknown[]; layout: { initial_data: unknown[] } } | null;
    }>("compile", {
      source: code.value,
      config: {
        base: riscvBase.value,
        extensions: Array.from(riscvExtensions.value),
      },
      codegenConfig: {
        text_base: compileConfigStore.textBase,
        data_base: compileConfigStore.dataBase,
      },
    });

    errorCount.value = result.error_count;
    errorOutput.value = result.error_output;

    if (result.error_count === 0 && result.codegen) {
      // Every instruction is a fixed 4 bytes, so the code size is just the entry count times 4.
      compileConfigStore.setCompiledSizes(result.codegen.entries.length * 4, result.codegen.layout.initial_data.length);
      await simulationStore.start(
        riscvBase.value,
        result.codegen,
        {
          isa: hierarchyConfigStore.isa,
          vmEnabled: hierarchyConfigStore.vmEnabled,
          pageSize: hierarchyConfigStore.pageSize,
          hierarchy: hierarchyConfigStore.hierarchy,
          tlb: hierarchyConfigStore.tlb,
          ram: hierarchyConfigStore.ram,
          disk: hierarchyConfigStore.disk,
          seed: hierarchyConfigStore.seed,
        },
        Array.from(breakpoints.value),
        Array.from(riscvExtensions.value),
      );
    }
  } catch (err) {
    errorCount.value = 1;
    errorOutput.value = typeof err === "string" ? err : String(err);
  } finally {
    isCompiling.value = false;
  }
}

async function handleOverwrite() {
  const source = pendingSource.value;
  closeModal();
  if (source) await performLoad(source);
}

async function handleSaveThenLoad() {
  const path = await save();
  if (!path) return; // the user cancelled the save dialog, so cancel the whole load too

  await writeTextFile(path, code.value);

  const source = pendingSource.value;
  closeModal();
  if (source) await performLoad(source);
}

function handleCancel() {
  closeModal();
}

const editorBodyRef = ref<HTMLDivElement | null>(null);
const currentLineHighlightRef = ref<HTMLDivElement | null>(null);
let unlistenDragDrop: (() => void) | null = null;
let unlistenSimEvents: (() => void) | null = null;

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
  document.addEventListener("click", handleDocumentClick);
  // This listens for files dragged in from outside the app, since the normal browser drop does not see them.
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

  unlistenSimEvents = await simulationStore.listenForEvents();

  measureLineHeight();
});

onBeforeUnmount(() => {
  document.removeEventListener("click", handleDocumentClick);
  unlistenDragDrop?.();
  unlistenSimEvents?.();
});

// Exposed only so tests can pick a language the app itself cannot pick yet.
defineExpose({ selectedLanguage });
</script>

<template>
  <div class="editor">
    <div class="editor-header">
      <IconButton
        size="sm"
        ariaLabel="New file"
        @click="handleNewFileClick"
      >
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

      <IconButton
        class="editor-open-file-button"
        size="sm"
        ariaLabel="Open file"
        @click="handleOpenFileClick"
      >
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path
            fill="currentColor"
            d="M1.5 3A1.5 1.5 0 0 1 3 1.5h3.379a1.5 1.5 0 0 1 1.06.44l1.122 1.12A1.5 1.5 0 0 0 9.62 3.5H13A1.5 1.5 0 0 1 14.5 5v7A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12V3Z"
          />
        </svg>
      </IconButton>

      <IconButton
        v-if="!simulationStore.isActive"
        size="sm"
        ariaLabel="Compile"
        :disabled="isCompiling"
        @click="handleStartClick"
      >
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path fill="currentColor" d="M4 2.5v11l10-5.5-10-5.5Z" />
        </svg>
      </IconButton>

      <template v-if="!simulationStore.isActive">
        <div class="editor-language-group" ref="optionsMenuRef">
          <SelectField v-model="selectedLanguage" :options="languageOptions" size="sm" aria-label="Language" />

          <IconButton
            size="sm"
            ariaLabel="Compilation options"
            @click.stop="toggleOptionsMenu"
          >
            <svg viewBox="0 0 16 16" width="16" height="16">
              <path fill="currentColor" d="M8 10.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z" />
              <path
                fill="currentColor"
                d="M8 1a1 1 0 0 1 1 1v.6a5.98 5.98 0 0 1 1.7.7l.42-.43a1 1 0 0 1 1.42 1.42l-.43.42c.32.5.56 1.08.7 1.7H13a1 1 0 1 1 0 2h-.19a5.98 5.98 0 0 1-.7 1.7l.43.42a1 1 0 1 1-1.42 1.42l-.42-.43a5.98 5.98 0 0 1-1.7.7V13a1 1 0 1 1-2 0v-.19a5.98 5.98 0 0 1-1.7-.7l-.42.43a1 1 0 1 1-1.42-1.42l.43-.42a5.98 5.98 0 0 1-.7-1.7H3a1 1 0 1 1 0-2h.19c.14-.62.38-1.2.7-1.7l-.43-.42A1 1 0 1 1 4.88 3.3l.42.43a5.98 5.98 0 0 1 1.7-.7V2a1 1 0 0 1 1-1Z"
              />
            </svg>
          </IconButton>

          <div v-if="isOptionsMenuOpen" class="editor-options-menu">
            <button
              v-for="base in (['RV32I', 'RV64I'] as const)"
              :key="base"
              type="button"
              class="editor-options-radio"
              :class="{ 'editor-options-radio-active': riscvBase === base }"
              @click="selectBase(base)"
            >
              {{ base }}
            </button>

            <div class="editor-options-divider"></div>

            <label
              v-for="extension in (['M', 'F', 'D', 'Q'] as const)"
              :key="extension"
              class="editor-options-checkbox"
            >
              <input
                type="checkbox"
                :checked="riscvExtensions.has(extension)"
                @change="toggleExtension(extension)"
              />
              {{ extension }}
            </label>
          </div>
        </div>
      </template>
      <SimulationToolbar v-else />
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
          :class="{ 'editor-gutter-line-current': simulationStore.currentLine === n }"
          @click="toggleBreakpoint(n)"
        >
          <span v-if="breakpoints.has(n)" class="editor-breakpoint-dot"></span>
          <span v-if="simulationStore.currentLine === n" class="editor-current-line-arrow">▶</span>
          {{ n }}
        </div>
      </div>
      <div
        class="editor-current-line-highlight"
        :style="currentLineHighlightStyle"
        ref="currentLineHighlightRef"
      ></div>
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

    <div v-if="languageHasCompileConfig" class="editor-compile-config">
      <CompileConfigForm :language="selectedLanguage" />
    </div>

    <ConfirmModal
      v-if="showConfirmModal"
      message="There is unsaved code in the editor. What do you want to do?"
      @save="handleSaveThenLoad"
      @discard="handleOverwrite"
      @cancel="handleCancel"
    />
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
  transition: border-color var(--theme-transition-duration) ease, background-color var(--theme-transition-duration) ease, box-shadow var(--theme-transition-duration) ease;
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

.editor-open-file-button {
  margin-right: auto;
}

.editor-language-group {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.3rem;
}

.editor-options-menu {
  position: absolute;
  top: calc(100% + 0.3rem);
  right: 0;
  min-width: 140px;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  padding: 0.5rem;
  border: 1px solid var(--secondary);
  border-radius: 8px;
  background: var(--background);
  box-shadow: var(--shadow);
  z-index: 5;
}

.editor-options-radio {
  padding: 0.3rem 0.5rem;
  border: 1px solid var(--secondary);
  border-radius: 6px;
  background: transparent;
  color: var(--text);
  font-family: inherit;
  font-size: 0.75rem;
  text-align: left;
  cursor: pointer;
}

.editor-options-radio-active {
  background: var(--secondary);
  color: var(--background);
}

.editor-options-divider {
  height: 1px;
  margin: 0.2rem 0;
  background: var(--secondary);
  opacity: 0.5;
}

.editor-options-checkbox {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.75rem;
  color: var(--text);
  cursor: pointer;
}

.editor-options-checkbox input[type="checkbox"] {
  accent-color: var(--accent);
}

.editor-body {
  position: relative;
  flex: 1 1 auto;
  display: flex;
  min-height: 0;
  overflow: hidden;
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

.editor-current-line-arrow {
  position: absolute;
  left: 2px;
  top: 50%;
  transform: translateY(-50%);
  font-size: 0.6rem;
  color: var(--accent);
}

.editor-gutter-line-current {
  color: var(--accent);
}

.editor-compile-config {
  flex: 0 0 auto;
  padding: 0.4rem 0.6rem;
  border-top: 1px solid var(--secondary);
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

</style>
