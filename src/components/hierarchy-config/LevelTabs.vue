<!-- src/components/hierarchy-config/LevelTabs.vue -->
<script setup lang="ts">
import { computed, ref, watch } from "vue";
import CacheConfigForm from "./CacheConfigForm.vue";
import TlbConfigForm from "./TlbConfigForm.vue";
import RamConfigForm from "./RamConfigForm.vue";
import DiskConfigForm from "./DiskConfigForm.vue";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { visibleTabs, pageWalkTargets } from "../../lib/hierarchy/derive";
import type { CacheConfig } from "../../lib/hierarchy/model";

const store = useHierarchyConfigStore();
const tabs = computed(() => visibleTabs(store.hierarchy, store.vmEnabled));
const activeTabId = ref<string | null>(tabs.value[0]?.id ?? null);

watch(tabs, (newTabs) => {
  if (!newTabs.some((t) => t.id === activeTabId.value)) {
    activeTabId.value = newTabs[0]?.id ?? null;
  }
});

const activeTab = computed(() => tabs.value.find((t) => t.id === activeTabId.value) ?? null);

const activeCacheConfig = computed(() => {
  if (!activeTab.value || activeTab.value.kind !== "cache") return null;
  const level = store.hierarchy[activeTab.value.levelIndex!];
  const chain = activeTab.value.chain!;
  const entry = chain === "unified" ? level.unified! : chain === "i" ? level.i! : level.d!;
  return { level, chain, entry };
});

function onCacheUpdate(patch: Partial<CacheConfig>) {
  if (!activeCacheConfig.value) return;
  store.updateCacheConfig(activeCacheConfig.value.level.id, activeCacheConfig.value.chain, patch);
}

function onApplyCacheConfigToAll() {
  if (!activeCacheConfig.value) return;
  store.applyCacheConfigToAllLevels(activeCacheConfig.value.entry.config);
}
</script>

<template>
  <div class="level-tabs">
    <div class="level-tabs-strip" role="tablist">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        role="tab"
        :aria-selected="tab.id === activeTabId"
        @click="activeTabId = tab.id"
      >
        {{ tab.label }}
      </button>
    </div>

    <div class="level-tabs-panel">
      <TlbConfigForm
        v-if="activeTab?.kind === 'tlb'"
        :tlb="store.tlb"
        :page-walk-targets="pageWalkTargets(store.hierarchy)"
        @update="store.updateTlbConfig"
      />
      <CacheConfigForm
        v-else-if="activeCacheConfig"
        :config="activeCacheConfig.entry.config"
        :cache-size-exp="activeCacheConfig.entry.exp"
        :is-instruction-cache="activeCacheConfig.chain === 'i'"
        :isa="store.isa"
        @update="onCacheUpdate"
        @apply-to-all="onApplyCacheConfigToAll"
      />
      <RamConfigForm
        v-else-if="activeTab?.kind === 'ram'"
        :config="store.ram"
        :hierarchy="store.hierarchy"
        :vm-enabled="store.vmEnabled"
        :isa="store.isa"
        @update="store.updateRamConfig"
      />
      <DiskConfigForm v-else-if="activeTab?.kind === 'disk'" :config="store.disk" @update="store.updateDiskConfig" />
    </div>
  </div>
</template>

<style scoped>
.level-tabs {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-height: 0;
}

.level-tabs-strip {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  padding: 0 0.875rem;
}

.level-tabs-strip button {
  min-height: 1.5rem;
  padding: 0.4rem 0.75rem;
  border: 1px solid var(--secondary);
  border-radius: 6px;
  background: var(--background);
  color: var(--text);
  font: inherit;
  cursor: pointer;
}

.level-tabs-strip button[aria-selected="true"] {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--background);
}

.level-tabs-strip button:hover {
  border-color: var(--accent);
}

.level-tabs-strip button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.level-tabs-panel {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0 0.875rem 0.875rem;
}

</style>
