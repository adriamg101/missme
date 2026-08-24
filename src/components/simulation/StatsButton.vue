<script setup lang="ts">
import IconButton from "../ui/IconButton.vue";
import { useSimulationStore } from "../../stores/simulation";

const store = useSimulationStore();
</script>

<template>
  <IconButton size="sm" ariaLabel="Stats" class="stats-button" @click="store.openView({ kind: 'stats' })">
    <svg viewBox="0 0 16 16" width="16" height="16">
      <path fill="currentColor" d="M2 13h2V7H2v6Zm4.5 0h2V3h-2v10Zm4.5 0h2V9h-2v4Z" />
    </svg>
  </IconButton>
</template>

<style scoped>
.stats-button {
  position: absolute;
  bottom: 0.5rem;
  left: 50%;
  transform: translateX(-50%);
}
</style>
