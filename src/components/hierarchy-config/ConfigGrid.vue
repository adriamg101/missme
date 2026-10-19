<!-- src/components/hierarchy-config/ConfigGrid.vue -->
<script setup lang="ts">
</script>

<template>
  <div class="config-grid">
    <slot />
  </div>
</template>

<style scoped>
.config-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
  gap: 1rem;
  align-content: start;
  width: 100%;
}
</style>
