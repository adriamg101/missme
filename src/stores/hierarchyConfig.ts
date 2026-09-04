import { defineStore } from "pinia";
import type { CacheConfig, DiskConfig, Hierarchy, ISA, RamConfig, TlbConfig } from "../lib/hierarchy/model";
import { defaultDiskConfig, defaultRamConfig } from "../lib/hierarchy/model";
import {
  createLevel,
  removeLevel as removeLevelOp,
  toHarvard,
  toVonNeumann,
  moveMarker as moveMarkerOp,
  clampToIsa,
  type MergeResult,
} from "../lib/hierarchy/operations";

function generateId(): string {
  return crypto.randomUUID();
}

interface HierarchyConfigState {
  isa: ISA;
  vmEnabled: boolean;
  pageSize: number;
  seed: number | null;
  hierarchy: Hierarchy;
  tlb: TlbConfig;
  ram: RamConfig;
  disk: DiskConfig;
}

export const useHierarchyConfigStore = defineStore("hierarchyConfig", {
  state: (): HierarchyConfigState => ({
    isa: "RV32I",
    vmEnabled: false,
    pageSize: 4096,
    seed: null,
    hierarchy: [],
    tlb: {
      pageWalkStart: "RAM",
      entries: 64,
      associativity: 4,
      replacement: "LRU",
    },
    ram: defaultRamConfig(),
    disk: defaultDiskConfig(),
  }),
  actions: {
    setIsa(isa: ISA, confirmFn: (message: string) => boolean = (msg) => confirm(msg)) {
      const result = clampToIsa(this.hierarchy, isa);
      if (result.requiresConfirmation) {
        const confirmed = confirmFn(
          "Changing the ISA forces two hierarchy levels to overlap at the same position. Apply anyway?",
        );
        if (!confirmed) return;
      }
      this.hierarchy = result.hierarchy;
      this.isa = isa;
    },
    setVmEnabled(value: boolean) {
      this.vmEnabled = value;
    },
    setPageSize(value: number) {
      this.pageSize = value;
    },
    setSeed(seed: number | null) {
      this.seed = seed;
    },
    createLevelAt(position: number) {
      const result = createLevel(this.hierarchy, position, generateId());
      if (result === null) return;
      this.hierarchy = result;
    },
    removeLevel(levelId: string) {
      this.hierarchy = removeLevelOp(this.hierarchy, levelId);
      if (this.tlb.pageWalkStart === levelId) {
        this.tlb.pageWalkStart = "RAM";
      }
    },
    convertToHarvard(levelId: string) {
      this.hierarchy = toHarvard(this.hierarchy, levelId);
    },
    convertToVonNeumann(levelId: string): MergeResult {
      const result = toVonNeumann(this.hierarchy, levelId);
      if (result.ok) this.hierarchy = result.hierarchy;
      return result;
    },
    moveMarker(levelId: string, chain: "i" | "d" | "unified", position: number) {
      const next = moveMarkerOp(this.hierarchy, levelId, chain, position, this.isa);
      this.hierarchy = next.map((level) => {
        if (level.id !== levelId) return level;
        const entry = chain === "unified" ? level.unified : chain === "i" ? level.i : level.d;
        if (!entry) return level;
        // 2 to the power of exp is the cache's total number of entries
        // (number of sets times associativity), and it doesn't depend on
        // block size. See diagram.ts's byteSizeLabel and the backend's
        // set-count calculation, both of which only use associativity, never
        // block size.
        const maxAssociativity = 2 ** entry.exp;
        if (entry.config.associativity <= maxAssociativity) return level;
        const clampedEntry = { ...entry, config: { ...entry.config, associativity: maxAssociativity } };
        if (chain === "unified") return { ...level, unified: clampedEntry };
        if (chain === "i") return { ...level, i: clampedEntry };
        return { ...level, d: clampedEntry };
      });
    },
    updateCacheConfig(levelId: string, chain: "i" | "d" | "unified", patch: Partial<CacheConfig>) {
      this.hierarchy = this.hierarchy.map((level) => {
        if (level.id !== levelId) return level;
        if (chain === "unified") return { ...level, unified: { ...level.unified!, config: { ...level.unified!.config, ...patch } } };
        if (chain === "i") return { ...level, i: { ...level.i!, config: { ...level.i!.config, ...patch } } };
        return { ...level, d: { ...level.d!, config: { ...level.d!.config, ...patch } } };
      });
    },
    applyCacheConfigToAllLevels(config: CacheConfig) {
      const applyToEntry = <T extends { exp: number; config: CacheConfig } | undefined>(entry: T): T => {
        if (!entry) return entry;
        const maxAssociativity = 2 ** entry.exp;
        const associativity = Math.min(config.associativity, maxAssociativity);
        return { ...entry, config: { ...config, associativity } };
      };
      this.hierarchy = this.hierarchy.map((level) => ({
        ...level,
        unified: applyToEntry(level.unified),
        i: applyToEntry(level.i),
        d: applyToEntry(level.d),
      }));
    },
    updateTlbConfig(patch: Partial<TlbConfig>) {
      this.tlb = { ...this.tlb, ...patch };
    },
    updateRamConfig(patch: Partial<RamConfig>) {
      this.ram = { ...this.ram, ...patch };
    },
    updateDiskConfig(patch: Partial<DiskConfig>) {
      this.disk = { ...this.disk, ...patch };
    },
  },
});
