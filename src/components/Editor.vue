<script setup lang="ts">
import { ref, computed } from "vue";

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
</script>

<template>
  <div class="editor">
    <div class="editor-header">
      <select v-model="selectedLanguage" class="editor-language-select">
        <option v-for="lang in languages" :key="lang" :value="lang">{{ lang }}</option>
      </select>
    </div>

    <div class="editor-body">
      <div class="editor-gutter" ref="gutterRef">
        <div v-for="n in lineCount" :key="n" class="editor-gutter-line">{{ n }}</div>
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
  </div>
</template>

<style scoped>
.editor {
  width: calc(100% - 0.5rem);
  height: calc(100% - 0.5rem);
  margin: 0.25rem;
  box-sizing: border-box;
  border-radius: 12px;
  border: 1px solid var(--secondary);
  background: var(--background);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.editor-header {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.4rem 0.6rem;
  border-bottom: 1px solid var(--secondary);
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
  padding: 0 0.5rem;
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
