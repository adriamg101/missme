<script setup lang="ts">
import MainLayout from "./components/MainLayout.vue"
import Editor from "./components/Editor.vue"
import TitleBar from "./components/TitleBar.vue"
import HierarchyConfigPanel from "./components/hierarchy-config/HierarchyConfigPanel.vue"
import HierarchyDiagram from "./components/hierarchy-config/HierarchyDiagram.vue"
</script>

<template>
  <div class="app-shell">
    <TitleBar />
    <div class="app-content">
      <MainLayout>
        <template #left>
          <Editor></Editor>
        </template>

        <template #top-right>
          <HierarchyDiagram></HierarchyDiagram>
        </template>

        <template #bottom-right>
          <HierarchyConfigPanel></HierarchyConfigPanel>
        </template>
      </MainLayout>
    </div>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}

.app-content {
  flex: 1 1 auto;
  min-height: 0;
}
</style>
