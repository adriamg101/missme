// src/lib/hierarchy/model.ts
export type ISA = "RV32I" | "RV64I";
export type ReplacementPolicy = "LRU" | "FIFO" | "Random" | "LFU" | "PLRU";
export type TlbReplacementPolicy = "LRU" | "FIFO" | "Random" | "LFU" | "PLRU";
export type WritePolicy = "write-through" | "write-back";
export type InclusionPolicy = "inclusive" | "exclusive" | "NINE";

export interface CacheConfig {
  blockSize: number;
  associativity: number;
  replacement: ReplacementPolicy;
  writePolicy: WritePolicy;
  writeAllocate: boolean;
  hitTime: number;
  // The chance from 0 to 100 that a line starts out dirty the first time it
  // is touched. Only matters for write back caches, since write through lines are never dirty.
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
  pageWalkStart: string; // either a Level.id, or the literal string "RAM"
  entries: number;
  associativity: number;
  replacement: TlbReplacementPolicy;
  latency: number;
}

export interface RamConfig {
  latency: number;
  // A plain power of two count, not a slider position, since RAM does not
  // appear on the hierarchy slider. Only meaningful when virtual memory is off.
  size: number;
  // The chance from 0 to 100 that a page starts out dirty the first time it
  // is touched. Only matters when virtual memory is turned on.
  dirtyPercent: number;
  pageTableLevels: number;
  // Which free frame a page fault picks when more than one is available,
  // either in order or a random one. Once all frames fill up, eviction always uses LRU.
  frameAllocation: "sequential" | "random";
  // How many frames the physical frame pool has, a plain power of two the
  // user picks directly. Only meaningful, and only shown, when VM is on.
  frames: number;
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
  return { latency: 100, size: 1024, dirtyPercent: 0, pageTableLevels: 1, frameAllocation: "sequential", frames: 4 };
}

export function defaultDiskConfig(): DiskConfig {
  return { latency: 10000 };
}

export function maxPositions(isa: ISA): number {
  return isa === "RV32I" ? 32 : 64;
}
