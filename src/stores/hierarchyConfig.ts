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
    seed: null,
    hierarchy: [],
    tlb: {
      pageSize: 4096,
      pageTableLevels: 1,
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
          "Cambiar la ISA obliga a solapar dos niveles de la jerarquía en la misma posición. ¿Aplicar de todas formas?",
        );
        if (!confirmed) return;
      }
      this.hierarchy = result.hierarchy;
      this.isa = isa;
    },
    setVmEnabled(value: boolean) {
      this.vmEnabled = value;
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
        const maxAssociativity = Math.max(1, Math.floor(2 ** entry.exp / entry.config.blockSize));
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
    updateTlbConfig(patch: Partial<TlbConfig>) {
      this.tlb = { ...this.tlb, ...patch };
    },
    updateRamConfig(patch: Partial<RamConfig>) {
      this.ram = { ...this.ram, ...patch };
    },
    updateDiskConfig(patch: Partial<DiskConfig>) {
      this.disk = { ...this.disk, ...patch };
    },
    resetToDefaults() {
      this.$reset();
    },
  },
});
