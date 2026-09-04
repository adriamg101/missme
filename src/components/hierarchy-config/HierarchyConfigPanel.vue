<script setup lang="ts">
import GlobalConfigBar from "./GlobalConfigBar.vue";
import LevelTabs from "./LevelTabs.vue";
</script>

<template>
  <div class="hierarchy-config-panel">
    <div class="hierarchy-config-panel-body">
      <GlobalConfigBar />
      <div class="hierarchy-config-panel-divider"></div>
      <LevelTabs />
    </div>
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
