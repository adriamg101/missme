// src/lib/hierarchy/model.ts
export type ISA = "RV32I" | "RV64I";
export type ReplacementPolicy = "LRU" | "FIFO" | "Random" | "LFU" | "PLRU";
export type TlbReplacementPolicy = "LRU" | "FIFO" | "Random" | "PLRU";
export type WritePolicy = "write-through" | "write-back";
export type InclusionPolicy = "inclusive" | "exclusive" | "NINE";

export interface CacheConfig {
  blockSize: number;
  associativity: number;
  replacement: ReplacementPolicy;
  writePolicy: WritePolicy;
  writeAllocate: boolean;
  hitTime: number;
  // Initial-condition knob, not a runtime-tracked bit: the probability (0-100)
  // that a line is already dirty the first time it's touched. Only meaningful
  // for write-back caches (write-through lines are never dirty). Resolved
  // lazily per line, seeded off the global Seed, by the backend when it runs.
  dirtyPercent: number;
}

export interface CacheEntry {
  exp: number;
  config: CacheConfig;
}

export interface Level {
  id: string;
  harvard: boolean;
  unified?: CacheEntry;
  i?: CacheEntry;
  d?: CacheEntry;
  inclusion: InclusionPolicy;
  busWidth: number;
}

export type Hierarchy = Level[];

export interface TlbConfig {
  pageSize: number;
  pageTableLevels: number;
  pageWalkStart: string; // a Level.id, or the literal "RAM"
  entries: number;
  associativity: number;
  replacement: TlbReplacementPolicy;
}

export interface RamConfig {
  latency: number;
  // A plain power-of-two count (same convention as CacheConfig.blockSize),
  // not a slider position — RAM does not live on the hierarchy slider.
  // Must be strictly greater than the largest cache level currently
  // configured; enforced by the form, not here.
  size: number;
  // Initial-condition knob: the probability (0-100) that a page is already
  // dirty the first time it's touched. A page property, not a per-cache-line
  // one — the TLB mirrors this rather than owning its own copy. Only
  // meaningful with virtual memory enabled.
  dirtyPercent: number;
}

export interface DiskConfig {
  latency: number;
}

export function defaultCacheConfig(): CacheConfig {
  return {
    blockSize: 64,
    associativity: 1,
    replacement: "LRU",
    writePolicy: "write-back",
    writeAllocate: true,
    hitTime: 1,
    dirtyPercent: 0,
  };
}

export function defaultRamConfig(): RamConfig {
  return { latency: 100, size: 1024, dirtyPercent: 0 };
}

export function defaultDiskConfig(): DiskConfig {
  return { latency: 10000 };
}

export function maxPositions(isa: ISA): number {
  return isa === "RV32I" ? 32 : 64;
}
