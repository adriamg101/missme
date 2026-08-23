<script setup lang="ts">
import { ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

const theme = ref<"light" | "dark">(
  (document.documentElement.getAttribute("data-theme") as "light" | "dark") ?? "dark"
);

function toggleTheme() {
  theme.value = theme.value === "light" ? "dark" : "light";
  document.documentElement.setAttribute("data-theme", theme.value);
}

function minimize() {
  appWindow.minimize();
}

function toggleMaximize() {
  appWindow.toggleMaximize();
}

function close() {
  appWindow.close();
}
</script>

<template>
  <div class="titlebar" data-tauri-drag-region>
    <span class="titlebar-title" data-tauri-drag-region>missme</span>

    <div class="titlebar-controls">
      <button class="titlebar-button theme-toggle" type="button" @click="toggleTheme" aria-label="Toggle theme">
        <svg v-if="theme === 'light'" viewBox="0 0 24 24" width="14" height="14">
          <path fill="currentColor" d="M12 4.5a1 1 0 0 1-1-1V2a1 1 0 0 1 2 0v1.5a1 1 0 0 1-1 1Zm0 17a1 1 0 0 1 1 1V24a1 1 0 0 1-2 0v-1.5a1 1 0 0 1 1-1ZM4.5 12a1 1 0 0 1-1 1H2a1 1 0 0 1 0-2h1.5a1 1 0 0 1 1 1Zm17 0a1 1 0 0 1 1-1H24a1 1 0 0 1 0 2h-1.5a1 1 0 0 1-1-1ZM6.34 6.34a1 1 0 0 1-1.41 0L3.87 5.28a1 1 0 0 1 1.41-1.41l1.06 1.06a1 1 0 0 1 0 1.41Zm12.02 12.02a1 1 0 0 1-1.41 0l-1.06-1.06a1 1 0 0 1 1.41-1.41l1.06 1.06a1 1 0 0 1 0 1.41ZM6.34 17.66a1 1 0 0 1 0 1.41L5.28 20.13a1 1 0 0 1-1.41-1.41l1.06-1.06a1 1 0 0 1 1.41 0ZM18.36 5.28a1 1 0 0 1 1.41 1.41l-1.06 1.06a1 1 0 0 1-1.41-1.41l1.06-1.06ZM12 7a5 5 0 1 1 0 10 5 5 0 0 1 0-10Z" />
        </svg>
        <svg v-else viewBox="0 0 24 24" width="14" height="14">
          <path fill="currentColor" d="M20.742 13.045a8.088 8.088 0 0 1-2.077.272c-4.492 0-8.131-3.639-8.131-8.131 0-1.416.362-2.746.999-3.904a10.05 10.05 0 0 0-8.688 9.958c0 5.554 4.502 10.056 10.056 10.056 4.415 0 8.166-2.845 9.518-6.798a8.084 8.084 0 0 1-1.677-1.453Z" />
        </svg>
      </button>

      <button class="titlebar-button" type="button" @click="minimize" aria-label="Minimize">
        <svg viewBox="0 0 12 12" width="12" height="12">
          <rect x="1" y="5.5" width="10" height="1" fill="currentColor" />
        </svg>
      </button>

      <button class="titlebar-button" type="button" @click="toggleMaximize" aria-label="Maximize">
        <svg viewBox="0 0 12 12" width="12" height="12">
          <rect x="1.5" y="1.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>

      <button class="titlebar-button titlebar-close" type="button" @click="close" aria-label="Close">
        <svg viewBox="0 0 12 12" width="12" height="12">
          <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.2" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 36px;
  flex: 0 0 36px;
  padding-left: 0.75rem;
  background: var(--background);
  color: var(--text);
  border-bottom: 1px solid var(--secondary);
  user-select: none;
}

.titlebar-title {
  font-size: 0.8rem;
  font-weight: 700;
}

.titlebar-controls {
  display: flex;
  align-items: stretch;
  height: 100%;
}

.titlebar-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--text);
  cursor: pointer;
}

.titlebar-button:hover {
  background: var(--secondary);
  opacity: 0.8;
}

.theme-toggle {
  width: 36px;
}

.titlebar-close:hover {
  background: #e81123;
  color: #fff;
  opacity: 1;
}
</style>
