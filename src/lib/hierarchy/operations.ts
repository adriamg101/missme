import type { CacheConfig, CacheEntry, Hierarchy, ISA, Level } from "./model";
import { defaultCacheConfig, maxPositions } from "./model";
import { chainPositions, firstChainViolation, insertionIndex, isPositionInsertable } from "./derive";

// The smallest selectable/reachable position — 0 is included so a cache
// with just a single entry (2^0 = 1) is reachable, same as any other
// power-of-two position.
export const MIN_POSITION = 0;

export function inheritedConfigFor(hierarchy: Hierarchy, insertIndex: number): CacheConfig {
  if (insertIndex === 0) return defaultCacheConfig();
  const prev = hierarchy[insertIndex - 1];
  const source = prev.harvard ? prev.d!.config : prev.unified!.config;
  return { ...source };
}

export function createLevel(hierarchy: Hierarchy, position: number, id: string): Hierarchy | null {
  if (!isPositionInsertable(hierarchy, position)) return null;
  const index = insertionIndex(hierarchy, position);
  const config = inheritedConfigFor(hierarchy, index);
  const newLevel: Level = {
    id,
    harvard: false,
    unified: { exp: position, config },
    inclusion: "inclusive",
    busWidth: config.blockSize,
  };
  const next = [...hierarchy];
  next.splice(index, 0, newLevel);
  return next;
}

export function removeLevel(hierarchy: Hierarchy, levelId: string): Hierarchy {
  return hierarchy.filter((level) => level.id !== levelId);
}

export function toHarvard(hierarchy: Hierarchy, levelId: string): Hierarchy {
  return hierarchy.map((level) => {
    if (level.id !== levelId || level.harvard) return level;
    const entry = level.unified!;
    return {
      ...level,
      harvard: true,
      unified: undefined,
      i: { exp: entry.exp, config: { ...entry.config } },
      d: { exp: entry.exp, config: { ...entry.config } },
    };
  });
}

export type MergeResult = { ok: true; hierarchy: Hierarchy } | { ok: false; conflictIndex: number };

export function toVonNeumann(hierarchy: Hierarchy, levelId: string): MergeResult {
  const index = hierarchy.findIndex((level) => level.id === levelId);
  const level = hierarchy[index];
  if (!level || !level.harvard) return { ok: true, hierarchy };

  const merged: Level = {
    ...level,
    harvard: false,
    unified: { exp: level.i!.exp, config: { ...level.i!.config } },
    i: undefined,
    d: undefined,
    busWidth: level.i!.config.blockSize,
  };
  const next = [...hierarchy];
  next[index] = merged;

  const violation = firstChainViolation(next, "d");
  if (violation !== null) return { ok: false, conflictIndex: violation };
  return { ok: true, hierarchy: next };
}

export function moveMarker(
  hierarchy: Hierarchy,
  levelId: string,
  chain: "i" | "d" | "unified",
  targetPosition: number,
  isa: ISA,
): Hierarchy {
  const index = hierarchy.findIndex((level) => level.id === levelId);
  if (index === -1) return hierarchy;

  const maxPos = maxPositions(isa) - 1;
  const chainsToCheck: Array<"i" | "d"> = chain === "unified" ? ["i", "d"] : [chain];

  let lower = MIN_POSITION;
  let upper = maxPos;
  for (const c of chainsToCheck) {
    const positions = chainPositions(hierarchy, c);
    const prev = index > 0 ? positions[index - 1] : -1;
    const next = index < positions.length - 1 ? positions[index + 1] : maxPos + 1;
    lower = Math.max(lower, prev + 1);
    upper = Math.min(upper, next - 1);
  }
  const clamped = Math.max(lower, Math.min(targetPosition, upper));

  const level = hierarchy[index];
  const nextHierarchy = [...hierarchy];
  if (chain === "unified") {
    nextHierarchy[index] = { ...level, unified: { ...level.unified!, exp: clamped } };
  } else if (chain === "i") {
    nextHierarchy[index] = { ...level, i: { ...level.i!, exp: clamped } };
  } else {
    nextHierarchy[index] = { ...level, d: { ...level.d!, exp: clamped } };
  }
  return nextHierarchy;
}

export interface ClampResult {
  hierarchy: Hierarchy;
  requiresConfirmation: boolean;
}

export function clampToIsa(hierarchy: Hierarchy, isa: ISA): ClampResult {
  const maxPos = maxPositions(isa) - 1;
  const clampEntry = (entry: CacheEntry): CacheEntry => (entry.exp > maxPos ? { ...entry, exp: maxPos } : entry);

  const next = hierarchy.map((level) => {
    if (level.harvard) {
      return { ...level, i: clampEntry(level.i!), d: clampEntry(level.d!) };
    }
    return { ...level, unified: clampEntry(level.unified!) };
  });

  const collided = firstChainViolation(next, "i") !== null || firstChainViolation(next, "d") !== null;
  return { hierarchy: next, requiresConfirmation: collided };
}
