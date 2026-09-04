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
  // This sets the starting condition, it doesn't track the actual state: the
  // chance (0-100) that a line is already dirty the first time it's touched.
  // Only matters for write-back caches, since write-through lines are never
  // dirty. The backend decides this per line when it runs, using the global Seed.
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
  // A plain power-of-two count (same convention as CacheConfig.blockSize),
  // not a slider position, since RAM doesn't appear on the hierarchy slider.
  // Only meaningful when virtual memory is off; VM mode uses `frames`
  // instead (see below).
  size: number;
  // This sets the starting condition: the chance (0-100) that a page is
  // already dirty the first time it's touched. It belongs to the page, not
  // to a specific cache line, so the TLB just copies this value instead of
  // having its own. Only matters when virtual memory is turned on.
  dirtyPercent: number;
  pageTableLevels: number;
  // Which free frame a page fault picks when more than one is available:
  // fill them in order, or pick a seeded-random one. Once every frame is
  // occupied, eviction always uses LRU regardless of this setting.
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
