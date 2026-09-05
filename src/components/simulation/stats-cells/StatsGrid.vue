<script setup lang="ts"></script>

<template>
  <div class="stats-grid">
    <slot />
  </div>
</template>

<style scoped>
.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(22rem, 1fr));
  gap: 1rem;
  align-content: start;
  width: 100%;
  transition: none;
}
</style>
