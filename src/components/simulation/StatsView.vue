<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";
import { visibleTabs, levelName, chainLabel, type TabDescriptor } from "../../lib/hierarchy/derive";
import StatsGrid from "./stats-cells/StatsGrid.vue";
import IntegerStat from "./stats-cells/IntegerStat.vue";
import DecimalStat from "./stats-cells/DecimalStat.vue";
import PercentStat from "./stats-cells/PercentStat.vue";
import StackedPercentStat from "./stats-cells/StackedPercentStat.vue";

interface BucketSnapshot {
  bucketKind: "tlb" | "cache" | "ram" | "disk";
  levelIndex: number | null;
  side: "unified" | "instruction" | "data" | null;
  accesses: number;
  hits: number;
  misses: number;
  cycles: number;
}
interface StatsSnapshot {
  instructionsCompleted: number;
  totalMemoryAccesses: number;
  topOfChainInstructionAccesses: number;
  topOfChainDataAccesses: number;
  totalCycles: number;
  amat: number;
  cpi: number;
  buckets: BucketSnapshot[];
}

const hierarchyStore = useHierarchyConfigStore();
const stats = ref<StatsSnapshot | null>(null);

type StatsTab = { kind: "global"; id: "global"; label: "Global" } | TabDescriptor;

const tabs = computed<StatsTab[]>(() => [
  { kind: "global", id: "global", label: "Global" },
  ...visibleTabs(hierarchyStore.hierarchy, hierarchyStore.vmEnabled),
]);
const activeTabId = ref<string>("global");
// Kept here, not inside the stat cell itself, so it survives switching away from the Global tab and back.
const excludedCycleShareLevels = ref<string[]>([]);
const activeTab = computed(() => tabs.value.find((t) => t.id === activeTabId.value) ?? tabs.value[0]);

async function fetchStats() {
  try {
    stats.value = await invoke<StatsSnapshot>("get_stats", {});
  } catch (error) {
    console.error("Failed to fetch stats:", error);
  }
}

function bucketFor(kind: "tlb" | "ram" | "disk"): BucketSnapshot | null {
  return stats.value?.buckets.find((b) => b.bucketKind === kind) ?? null;
}

function cacheBucketFor(levelIndex: number, side: "unified" | "instruction" | "data"): BucketSnapshot | null {
  return stats.value?.buckets.find((b) => b.bucketKind === "cache" && b.levelIndex === levelIndex && b.side === side) ?? null;
}

const activeCacheBucket = computed<BucketSnapshot | null>(() => {
  if (!activeTab.value || activeTab.value.kind !== "cache") return null;
  const side = activeTab.value.chain === "i" ? "instruction" : activeTab.value.chain === "d" ? "data" : "unified";
  return cacheBucketFor(activeTab.value.levelIndex!, side);
});

function localRate(bucket: BucketSnapshot | null, part: "hits" | "misses"): number {
  if (!bucket) return 0;
  const total = bucket.hits + bucket.misses;
  if (total === 0) return 0;
  return bucket[part] / total;
}

function globalRate(bucket: BucketSnapshot | null): number {
  if (!bucket || !stats.value) return 0;
  const denominator =
    bucket.side === "instruction"
      ? stats.value.topOfChainInstructionAccesses
      : bucket.side === "data"
        ? stats.value.topOfChainDataAccesses
        : stats.value.totalMemoryAccesses;
  return denominator === 0 ? 0 : bucket.hits / denominator;
}

function cycleShare(bucket: BucketSnapshot | null): number {
  if (!bucket || !stats.value || stats.value.totalCycles === 0) return 0;
  return bucket.cycles / stats.value.totalCycles;
}

const globalSegments = computed(() => {
  if (!stats.value) return [];
  const totalCycles = stats.value.totalCycles;
  const bucketSegments = stats.value.buckets.map((bucket) => ({
    label: bucketLabel(bucket),
    value: totalCycles === 0 ? 0 : bucket.cycles / totalCycles,
  }));
  const bucketCycles = stats.value.buckets.reduce((sum, bucket) => sum + bucket.cycles, 0);
  const executeCycles = totalCycles - bucketCycles;
  const executeSegment = {
    label: "Execute",
    value: totalCycles === 0 ? 0 : executeCycles / totalCycles,
  };
  return [executeSegment, ...bucketSegments];
});

function bucketLabel(bucket: BucketSnapshot): string {
  if (bucket.bucketKind === "tlb") return "TLB";
  if (bucket.bucketKind === "ram") return "RAM";
  if (bucket.bucketKind === "disk") return "Disk";
  if (bucket.levelIndex === null) return "";
  if (bucket.side === "instruction") return chainLabel(hierarchyStore.hierarchy, bucket.levelIndex, "i");
  if (bucket.side === "data") return chainLabel(hierarchyStore.hierarchy, bucket.levelIndex, "d");
  return levelName(hierarchyStore.hierarchy, bucket.levelIndex);
}

function hitLatencyFor(levelIndex: number, side: "unified" | "instruction" | "data"): number {
  const level = hierarchyStore.hierarchy[levelIndex];
  if (!level) return 0;
  const entry = side === "instruction" ? level.i : side === "data" ? level.d : level.unified;
  return entry?.config.hitTime ?? 0;
}

let unlisten: (() => void) | null = null;
let disposed = false;

onMounted(async () => {
  await fetchStats();
  const stopListening = await listen<Record<string, unknown>>("sim:event", (event) => {
    const kind = event.payload.kind;
    if (kind === "cache_access" || kind === "ram_access" || kind === "disk_access" || kind === "tlb_access" || kind === "instruction_complete") {
      fetchStats();
    }
  });
  if (disposed) stopListening();
  else unlisten = stopListening;
});

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});
</script>

<template>
  <div class="simulation-stats-view">
    <div class="simulation-stats-tabs" role="tablist">
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

    <div v-if="stats" class="simulation-stats-panel">
      <template v-if="activeTab?.kind === 'global'">
        <div class="simulation-stats-highlight">
          <StackedPercentStat label="Cycle share by level" :segments="globalSegments" v-model:excluded="excludedCycleShareLevels" />
        </div>
        <StatsGrid>
          <IntegerStat label="Instructions executed" :value="stats.instructionsCompleted" />
          <IntegerStat label="Total memory accesses" :value="stats.totalMemoryAccesses" />
          <DecimalStat label="AMAT" :value="stats.amat" :decimals="2" />
          <DecimalStat label="CPI" :value="stats.cpi" :decimals="2" />
          <IntegerStat label="Total cycles" :value="stats.totalCycles" />
        </StatsGrid>
      </template>

      <StatsGrid v-else-if="activeTab?.kind === 'tlb'">
        <IntegerStat label="Accesses" :value="bucketFor('tlb')?.accesses ?? 0" />
        <IntegerStat label="Hits" :value="bucketFor('tlb')?.hits ?? 0" />
        <IntegerStat label="Misses" :value="bucketFor('tlb')?.misses ?? 0" />
        <PercentStat label="Hit rate" :value="localRate(bucketFor('tlb'), 'hits')" />
        <PercentStat label="Miss rate" :value="localRate(bucketFor('tlb'), 'misses')" />
        <IntegerStat label="Cycles consumed" :value="bucketFor('tlb')?.cycles ?? 0" />
        <PercentStat label="% of cycles" :value="cycleShare(bucketFor('tlb'))" />
      </StatsGrid>

      <StatsGrid v-else-if="activeTab?.kind === 'cache'">
        <IntegerStat label="Accesses" :value="activeCacheBucket?.accesses ?? 0" />
        <IntegerStat label="Hits" :value="activeCacheBucket?.hits ?? 0" />
        <IntegerStat label="Misses" :value="activeCacheBucket?.misses ?? 0" />
        <PercentStat label="Local hit rate" :value="localRate(activeCacheBucket, 'hits')" />
        <PercentStat label="Local miss rate" :value="localRate(activeCacheBucket, 'misses')" />
        <template v-if="activeTab.levelIndex !== 0">
          <PercentStat label="Global hit rate" :value="globalRate(activeCacheBucket)" />
          <PercentStat label="Global miss rate" :value="1 - globalRate(activeCacheBucket)" />
        </template>
        <IntegerStat
          label="Hit latency (cycles)"
          :value="hitLatencyFor(activeTab.levelIndex!, activeTab.chain === 'i' ? 'instruction' : activeTab.chain === 'd' ? 'data' : 'unified')"
        />
        <IntegerStat label="Cycles consumed" :value="activeCacheBucket?.cycles ?? 0" />
        <PercentStat label="% of cycles" :value="cycleShare(activeCacheBucket)" />
      </StatsGrid>

      <StatsGrid v-else-if="activeTab?.kind === 'ram' && hierarchyStore.vmEnabled">
        <IntegerStat label="RAM steps" :value="bucketFor('ram')?.accesses ?? 0" />
        <IntegerStat label="Page translations" :value="(bucketFor('ram')?.hits ?? 0) + (bucketFor('ram')?.misses ?? 0)" />
        <IntegerStat label="Page hits" :value="bucketFor('ram')?.hits ?? 0" />
        <IntegerStat label="Page faults" :value="bucketFor('ram')?.misses ?? 0" />
        <PercentStat label="Page hit rate" :value="localRate(bucketFor('ram'), 'hits')" />
        <PercentStat label="Page fault rate" :value="localRate(bucketFor('ram'), 'misses')" />
        <IntegerStat label="Cycles consumed" :value="bucketFor('ram')?.cycles ?? 0" />
        <PercentStat label="% of cycles" :value="cycleShare(bucketFor('ram'))" />
      </StatsGrid>
      <StatsGrid v-else-if="activeTab?.kind === 'ram'">
        <IntegerStat label="Accesses" :value="bucketFor('ram')?.accesses ?? 0" />
        <IntegerStat label="Cycles consumed" :value="bucketFor('ram')?.cycles ?? 0" />
        <PercentStat label="% of cycles" :value="cycleShare(bucketFor('ram'))" />
      </StatsGrid>

      <StatsGrid v-else-if="activeTab?.kind === 'disk'">
        <IntegerStat label="Accesses" :value="bucketFor('disk')?.accesses ?? 0" />
        <IntegerStat label="Cycles consumed" :value="bucketFor('disk')?.cycles ?? 0" />
        <PercentStat label="% of cycles" :value="cycleShare(bucketFor('disk'))" />
      </StatsGrid>
    </div>
  </div>
</template>

<style scoped>
.simulation-stats-view {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0.75rem;
  height: 100%;
  box-sizing: border-box;
  color: var(--text);
}

.simulation-stats-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.simulation-stats-tabs button {
  min-height: 1.5rem;
  padding: 0.4rem 0.75rem;
  border: 1px solid var(--secondary);
  border-radius: 6px;
  background: var(--background);
  color: var(--text);
  font: inherit;
  cursor: pointer;
}

.simulation-stats-tabs button[aria-selected="true"] {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--background);
}

.simulation-stats-tabs button:hover {
  border-color: var(--accent);
}

.simulation-stats-panel {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.simulation-stats-highlight {
  flex: 0 0 auto;
}
</style>
