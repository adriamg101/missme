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

export function defaultCacheConfig(): CacheConfig {
  return {
    blockSize: 64,
    associativity: 1,
    replacement: "LRU",
    writePolicy: "write-back",
    writeAllocate: true,
    hitTime: 1,
  };
}

export function maxPositions(isa: ISA): number {
  return isa === "RV32I" ? 32 : 64;
}
