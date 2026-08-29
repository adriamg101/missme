<!-- src/components/ui/IconButton.vue -->
<script setup lang="ts">
withDefaults(defineProps<{ ariaLabel: string; size?: "sm" | "md" | "lg" }>(), { size: "md" });
</script>

<template>
  <button type="button" class="icon-button" :class="`size-${size}`" :aria-label="ariaLabel" :title="ariaLabel">
    <slot />
  </button>
</template>

<style scoped>
.icon-button {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  padding: 2px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text);
  cursor: pointer;
}

.icon-button:hover {
  background: var(--secondary);
  opacity: 0.8;
}

.icon-button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.icon-button.size-sm {
  width: var(--control-height-sm);
  height: var(--control-height-sm);
}

.icon-button.size-md {
  width: var(--control-height-md);
  height: var(--control-height-md);
}

.icon-button.size-lg {
  width: var(--control-height-lg);
  height: var(--control-height-lg);
}

.icon-button :deep(svg) {
  width: 100%;
  height: 100%;
}
</style>
