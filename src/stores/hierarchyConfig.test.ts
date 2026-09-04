import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createApp } from "vue";
import { useHierarchyConfigStore } from "./hierarchyConfig";
import { persistPlugin } from "./persistPlugin";

// Pinia only actually turns on a plugin registered with `pinia.use()` once
// the store is installed into a Vue app. Before that, it just remembers the
// plugin to install later. main.ts does this installation when it starts
// the real app, so tests need a throwaway app of their own to get the same
// real plugin behavior.
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
    store.createLevelAt(40); // only valid under RV64I, since RV32I's max is 31, so start with RV64I
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

    // Shrinking to exp 2 means the max associativity is 2**2 = 4 entries, so
    // 16 gets brought down to 4. The max associativity doesn't depend on
    // block size, it only depends on the entry count.
    store.moveMarker(id, "unified", 2);
    expect(store.hierarchy[0].unified!.exp).toBe(2);
    expect(store.hierarchy[0].unified!.config.associativity).toBe(4);
  });

  it("applyCacheConfigToAllLevels copies the config to every cache entry, clamping associativity per level", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(10); // exp 10 means max associativity is 1024
    store.createLevelAt(2); // exp 2 means max associativity is 4, a smaller cache
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
    // exp 2 means max associativity is 2**2 = 4, so the requested 16 gets brought down to 4.
    const smallLevel = store.hierarchy.find((l) => l.id === smallId)!;
    expect(smallLevel.unified!.config.associativity).toBe(4);
    expect(smallLevel.unified!.config.replacement).toBe("FIFO");
  });

  it("setIsa applies the clamp (colliding two levels) when the confirm callback returns true", () => {
    const store = useHierarchyConfigStore();
    store.createLevelAt(31); // valid under RV32I, stays at 31 after clamping
    store.setIsa("RV64I"); // no conflict here, so the default confirm is fine
    store.createLevelAt(40); // a second level, only valid under RV64I

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
