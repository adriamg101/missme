import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createApp } from "vue";
import { useHierarchyConfigStore } from "./hierarchyConfig";
import { persistPlugin } from "./persistPlugin";

// Pinia only actually applies plugins registered via `pinia.use()` once the
// pinia instance is installed into a Vue app (`app.use(pinia)`) — until
// then, `.use()` just queues the plugin in an internal `toBeInstalled` list
// (see node_modules/pinia/dist/pinia.js, `createPinia`/`use`). main.ts does
// this installation as part of mounting the real app; tests need an
// equivalent (throwaway) app to get the same real plugin wiring.
function freshPersistedPinia() {
  const pinia = createPinia();
  pinia.use(persistPlugin);
  createApp({}).use(pinia);
  return pinia;
}

beforeEach(() => {
  setActivePinia(freshPersistedPinia());
  localStorage.clear();
});

describe("useHierarchyConfigStore", () => {
  it("starts empty with sane defaults", () => {
    const store = useHierarchyConfigStore();
    expect(store.hierarchy).toEqual([]);
    expect(store.isa).toBe("RV32I");
    expect(store.vmEnabled).toBe(false);
    expect(store.seed).toBeNull();
  });

  it("creates a level via createLevelAt and it shows up as L1", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(10);
    expect(store.hierarchy).toHaveLength(1);
    expect(store.hierarchy[0].unified!.exp).toBe(10);
  });

  it("rejects an insert with no gap without throwing or changing state", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.createLevelAt(6);
    expect(store.hierarchy).toHaveLength(2);
    const before = store.hierarchy;
    store.createLevelAt(5);
    expect(store.hierarchy).toBe(before);
  });

  it("convertToVonNeumann surfaces a conflict without mutating state", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(2);
    store.createLevelAt(6);
    store.convertToHarvard(store.hierarchy[1].id);
    store.moveMarker(store.hierarchy[1].id, "i", 25);
    store.convertToHarvard(store.hierarchy[0].id);
    store.moveMarker(store.hierarchy[0].id, "i", 10);
    const before = store.hierarchy;
    const result = store.convertToVonNeumann(store.hierarchy[0].id);
    expect(result.ok).toBe(false);
    expect(store.hierarchy).toBe(before);
  });

  it("updateCacheConfig patches only the targeted level/chain", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(10);
    const id = store.hierarchy[0].id;
    store.updateCacheConfig(id, "unified", { blockSize: 32 });
    expect(store.hierarchy[0].unified!.config.blockSize).toBe(32);
  });

  it("setIsa clamps existing markers to the new ISA range", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.createLevelAt(40); // valid under RV64I default? RV32I default max is 31, so seed via RV64I first
    store.setIsa("RV64I");
    store.createLevelAt(40);
    store.setIsa("RV32I");
    const clamped = store.hierarchy.find((l) => l.unified!.exp > 31);
    expect(clamped).toBeUndefined();
  });

  it("persists to localStorage on mutation and hydrates a fresh store from it", () => {
    const store = useHierarchyConfigStore();
    store.setVmEnabled(true);
    store.createLevelAt(12);

    setActivePinia(freshPersistedPinia());
    const rehydrated = useHierarchyConfigStore();
    expect(rehydrated.vmEnabled).toBe(true);
    expect(rehydrated.hierarchy).toHaveLength(1);
  });

  it("generates unique ids across a reload, so a rehydrated level and a newly created one never collide", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    const oldId = store.hierarchy[0].id;

    setActivePinia(freshPersistedPinia());
    const rehydrated = useHierarchyConfigStore();
    rehydrated.createLevelAt(20);
    const newId = rehydrated.hierarchy.find((l) => l.id !== oldId)!.id;

    expect(newId).not.toBe(oldId);

    rehydrated.removeLevel(newId);
    expect(rehydrated.hierarchy.map((l) => l.id)).toEqual([oldId]);

    rehydrated.createLevelAt(20);
    const newId2 = rehydrated.hierarchy.find((l) => l.id !== oldId)!.id;
    rehydrated.removeLevel(oldId);
    expect(rehydrated.hierarchy.map((l) => l.id)).toEqual([newId2]);
  });

  it("resets tlb.pageWalkStart to RAM when its target level is removed", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(5);
    store.createLevelAt(20);
    const secondId = store.hierarchy[1].id;
    store.updateTlbConfig({ pageWalkStart: secondId });
    expect(store.tlb.pageWalkStart).toBe(secondId);

    store.removeLevel(secondId);
    expect(store.tlb.pageWalkStart).toBe("RAM");
  });

  it("clamps associativity down when moveMarker shrinks the cache size below the current max", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(10); // exp 10 -> max associativity 2**10 = 1024 entries
    const id = store.hierarchy[0].id;
    store.updateCacheConfig(id, "unified", { blockSize: 64, associativity: 16 });
    expect(store.hierarchy[0].unified!.config.associativity).toBe(16);

    // shrink to exp 2 -> max associativity 2**2 = 4 entries, clamps 16 down to 4.
    // Max associativity is independent of blockSize — it's purely the
    // entry count (numSets * associativity), per diagram.ts/backend.
    store.moveMarker(id, "unified", 2);
    expect(store.hierarchy[0].unified!.exp).toBe(2);
    expect(store.hierarchy[0].unified!.config.associativity).toBe(4);
  });

  it("applyCacheConfigToAllLevels copies the config to every cache entry, clamping associativity per level", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(10); // exp 10 -> max associativity 1024
    store.createLevelAt(2); // exp 2 -> max associativity 4, smaller cache
    const bigLevel = store.hierarchy.find((l) => l.unified!.exp === 10)!;
    const smallId = store.hierarchy.find((l) => l.unified!.exp === 2)!.id;

    store.applyCacheConfigToAllLevels({
      blockSize: 64,
      associativity: 16,
      replacement: "FIFO",
      writePolicy: "write-through",
      writeAllocate: false,
      hitTime: 3,
      dirtyPercent: 20,
    });

    const bigLevelAfter = store.hierarchy.find((l) => l.id === bigLevel.id)!;
    expect(bigLevelAfter.unified!.config).toEqual({
      blockSize: 64,
      associativity: 16,
      replacement: "FIFO",
      writePolicy: "write-through",
      writeAllocate: false,
      hitTime: 3,
      dirtyPercent: 20,
    });
    // exp 2 -> max associativity 2**2 = 4, clamps the requested 16 down to 4.
    const smallLevel = store.hierarchy.find((l) => l.id === smallId)!;
    expect(smallLevel.unified!.config.associativity).toBe(4);
    expect(smallLevel.unified!.config.replacement).toBe("FIFO");
  });

  it("setIsa applies the clamp (colliding two levels) when the confirm callback returns true", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(31); // valid under RV32I, stays at 31 when clamped
    store.setIsa("RV64I"); // no collision here, safe with default confirm
    store.createLevelAt(40); // second level, only valid under RV64I

    const confirmFn = () => true;
    store.setIsa("RV32I", confirmFn);
    expect(store.isa).toBe("RV32I");
    expect(store.hierarchy.map((l) => l.unified!.exp)).toEqual([31, 31]);
  });

  it("setIsa leaves the hierarchy and isa unchanged when the confirm callback returns false", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(31);
    store.setIsa("RV64I");
    store.createLevelAt(40);
    const before = store.hierarchy;

    const confirmFn = () => false;
    store.setIsa("RV32I", confirmFn);
    expect(store.isa).toBe("RV64I");
    expect(store.hierarchy).toBe(before);
  });

});
