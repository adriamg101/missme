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
        @update="onCacheUpdate"
      />
      <RamConfigForm v-else-if="activeTab?.kind === 'ram'" />
      <DiskConfigForm v-else-if="activeTab?.kind === 'disk'" />
    </div>
  </div>
</template>
