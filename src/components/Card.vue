<script setup lang="ts">
</script>

<template>
  <div class="card">
    <slot></slot>
  </div>
</template>

<style scoped>
.card {
  width: calc(100% - 0.5rem);
  height: calc(100% - 0.5rem);
  margin: 0.25rem;
  padding: 1rem;
  box-sizing: border-box;
  border-radius: 12px;
  border: 1px solid var(--secondary);
  background: var(--background);
}
</style>
