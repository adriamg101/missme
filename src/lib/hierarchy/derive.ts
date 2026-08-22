// src/lib/hierarchy/derive.ts
import type { CacheEntry, Hierarchy, Level } from "./model";

export function chainEntry(level: Level, chain: "i" | "d"): CacheEntry {
  if (level.harvard) return chain === "i" ? level.i! : level.d!;
  return level.unified!;
}

export function chainPositions(hierarchy: Hierarchy, chain: "i" | "d"): number[] {
  return hierarchy.map((level) => chainEntry(level, chain).exp);
}

export function firstChainViolation(hierarchy: Hierarchy, chain: "i" | "d"): number | null {
  const positions = chainPositions(hierarchy, chain);
  for (let idx = 1; idx < positions.length; idx++) {
    if (positions[idx] <= positions[idx - 1]) return idx;
  }
  return null;
}

export function chainIsValid(hierarchy: Hierarchy, chain: "i" | "d"): boolean {
  return firstChainViolation(hierarchy, chain) === null;
}

export function levelName(_hierarchy: Hierarchy, index: number): string {
  return `L${index + 1}`;
}

export function chainLabel(hierarchy: Hierarchy, index: number, chain: "i" | "d"): string {
  const level = hierarchy[index];
  const base = levelName(hierarchy, index);
  if (!level.harvard) return base;
  return chain === "i" ? `${base}i` : `${base}d`;
}

const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

export function sizeLabel(exp: number): string {
  const unitIndex = Math.min(Math.floor(exp / 10), SIZE_UNITS.length - 1);
  const value = 2 ** (exp - unitIndex * 10);
  return `${value} ${SIZE_UNITS[unitIndex]}`;
}

export interface OccupiedSpan {
  levelIndex: number;
  from: number;
  to: number;
}

export function occupiedSpans(hierarchy: Hierarchy): OccupiedSpan[] {
  return hierarchy.map((level, levelIndex) => {
    if (!level.harvard) {
      const p = level.unified!.exp;
      return { levelIndex, from: p, to: p };
    }
    const a = level.i!.exp;
    const b = level.d!.exp;
    return { levelIndex, from: Math.min(a, b), to: Math.max(a, b) };
  });
}

export function isPositionInsertable(hierarchy: Hierarchy, position: number): boolean {
  return !occupiedSpans(hierarchy).some((span) => position >= span.from && position <= span.to);
}

export function insertionIndex(hierarchy: Hierarchy, position: number): number {
  let index = 0;
  for (const span of occupiedSpans(hierarchy)) {
    if (span.to < position) index++;
  }
  return index;
}

export interface TabDescriptor {
  kind: "tlb" | "cache" | "ram" | "disk";
  id: string;
  label: string;
  levelIndex?: number;
  chain?: "i" | "d" | "unified";
}

export function visibleTabs(hierarchy: Hierarchy, vmEnabled: boolean): TabDescriptor[] {
  const tabs: TabDescriptor[] = [];
  if (vmEnabled) tabs.push({ kind: "tlb", id: "tlb", label: "TLB" });
  hierarchy.forEach((level, index) => {
    if (level.harvard) {
      tabs.push({ kind: "cache", id: `${level.id}-i`, label: chainLabel(hierarchy, index, "i"), levelIndex: index, chain: "i" });
      tabs.push({ kind: "cache", id: `${level.id}-d`, label: chainLabel(hierarchy, index, "d"), levelIndex: index, chain: "d" });
    } else {
      tabs.push({ kind: "cache", id: level.id, label: levelName(hierarchy, index), levelIndex: index, chain: "unified" });
    }
  });
  if (vmEnabled) {
    tabs.push({ kind: "ram", id: "ram", label: "RAM" });
    tabs.push({ kind: "disk", id: "disk", label: "DISK" });
  }
  return tabs;
}

export interface PageWalkTarget {
  id: string;
  label: string;
}

export function pageWalkTargets(hierarchy: Hierarchy): PageWalkTarget[] {
  const targets = hierarchy.slice(1).map((level, offset) => ({
    id: level.id,
    label: levelName(hierarchy, offset + 1),
  }));
  targets.push({ id: "RAM", label: "RAM" });
  return targets;
}
