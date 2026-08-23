// src/lib/hierarchy/derive.test.ts
import { describe, it, expect } from "vitest";
import type { Hierarchy, Level } from "./model";
import { defaultCacheConfig } from "./model";
import {
  chainPositions,
  firstChainViolation,
  chainIsValid,
  levelName,
  chainLabel,
  sizeLabel,
  occupiedSpans,
  isPositionInsertable,
  insertionIndex,
  visibleTabs,
  pageWalkTargets,
  maxCacheSize,
  maxCacheByteCapacity,
} from "./derive";

function unifiedLevel(id: string, exp: number): Level {
  return { id, harvard: false, unified: { exp, config: defaultCacheConfig() }, inclusion: "inclusive", busWidth: 64 };
}

function harvardLevel(id: string, iExp: number, dExp: number): Level {
  return {
    id,
    harvard: true,
    i: { exp: iExp, config: defaultCacheConfig() },
    d: { exp: dExp, config: defaultCacheConfig() },
    inclusion: "inclusive",
    busWidth: 64,
  };
}

describe("sizeLabel", () => {
  it("formats base-scale counts with no unit suffix", () => {
    expect(sizeLabel(0)).toBe("1");
    expect(sizeLabel(4)).toBe("16");
  });
  it("formats K/M/G exactly on unit boundaries", () => {
    expect(sizeLabel(10)).toBe("1 K");
    expect(sizeLabel(20)).toBe("1 M");
    expect(sizeLabel(30)).toBe("1 G");
  });
  it("formats non-boundary exponents within a unit", () => {
    expect(sizeLabel(13)).toBe("8 K");
    expect(sizeLabel(25)).toBe("32 M");
  });
});

describe("chain positions and validity", () => {
  it("reads unified levels into both chains", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 15)];
    expect(chainPositions(h, "i")).toEqual([5, 15]);
    expect(chainPositions(h, "d")).toEqual([5, 15]);
  });

  it("reads harvard levels per-chain independently, allowing i/d to cross", () => {
    const h: Hierarchy = [harvardLevel("a", 10, 20)];
    expect(chainPositions(h, "i")).toEqual([10]);
    expect(chainPositions(h, "d")).toEqual([20]);
  });

  it("detects a strictly-increasing violation and reports its index", () => {
    const h: Hierarchy = [unifiedLevel("a", 10), unifiedLevel("b", 5)];
    expect(firstChainViolation(h, "i")).toBe(1);
    expect(chainIsValid(h, "i")).toBe(false);
  });

  it("is valid when strictly increasing", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 10)];
    expect(firstChainViolation(h, "i")).toBeNull();
    expect(chainIsValid(h, "i")).toBe(true);
  });
});

describe("names and labels", () => {
  it("derives L{n} names purely from position", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), harvardLevel("b", 10, 20)];
    expect(levelName(h, 0)).toBe("L1");
    expect(levelName(h, 1)).toBe("L2");
    expect(chainLabel(h, 0, "i")).toBe("L1");
    expect(chainLabel(h, 1, "i")).toBe("L2i");
    expect(chainLabel(h, 1, "d")).toBe("L2d");
  });
});

describe("occupied spans and insertability", () => {
  const h: Hierarchy = [unifiedLevel("a", 5), harvardLevel("b", 10, 20), unifiedLevel("c", 25)];

  it("computes one-point spans for unified and range spans for harvard", () => {
    expect(occupiedSpans(h)).toEqual([
      { levelIndex: 0, from: 5, to: 5 },
      { levelIndex: 1, from: 10, to: 20 },
      { levelIndex: 2, from: 25, to: 25 },
    ]);
  });

  it("rejects insertion inside a harvard level's span", () => {
    expect(isPositionInsertable(h, 15)).toBe(false);
  });

  it("accepts insertion between two levels and computes the right index", () => {
    expect(isPositionInsertable(h, 22)).toBe(true);
    expect(insertionIndex(h, 22)).toBe(2);
  });

  it("accepts insertion before the first level", () => {
    expect(isPositionInsertable(h, 1)).toBe(true);
    expect(insertionIndex(h, 1)).toBe(0);
  });
});

describe("visibleTabs", () => {
  it("always includes RAM but omits TLB/DISK when VM is disabled", () => {
    const h: Hierarchy = [unifiedLevel("a", 5)];
    const tabs = visibleTabs(h, false);
    expect(tabs.map((t) => t.kind)).toEqual(["cache", "ram"]);
  });

  it("includes TLB/RAM/DISK in the right order when VM is enabled", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), harvardLevel("b", 10, 20)];
    const tabs = visibleTabs(h, true);
    expect(tabs.map((t) => t.label)).toEqual(["TLB", "L1", "L2i", "L2d", "RAM", "DISK"]);
  });
});

describe("pageWalkTargets", () => {
  it("excludes L1 and always offers RAM", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 15), unifiedLevel("c", 25)];
    expect(pageWalkTargets(h)).toEqual([
      { id: "b", label: "L2" },
      { id: "c", label: "L3" },
      { id: "RAM", label: "RAM" },
    ]);
  });
});

describe("maxCacheSize", () => {
  it("returns 1 for an empty hierarchy", () => {
    expect(maxCacheSize([])).toBe(1);
  });

  it("returns the largest unified level's size", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 15)];
    expect(maxCacheSize(h)).toBe(2 ** 15);
  });

  it("considers both i and d sides of a harvard level", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), harvardLevel("b", 10, 20)];
    expect(maxCacheSize(h)).toBe(2 ** 20);
  });
});

describe("maxCacheByteCapacity", () => {
  it("returns 1 for an empty hierarchy", () => {
    expect(maxCacheByteCapacity([])).toBe(1);
  });

  it("returns the largest unified level's byte capacity (entries * block size)", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 10)];
    // 2^10 entries * 64 bytes/entry = 65536
    expect(maxCacheByteCapacity(h)).toBe(2 ** 10 * 64);
  });

  it("considers both i and d sides of a harvard level", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), harvardLevel("b", 8, 12)];
    // 2^12 entries * 64 bytes/entry = 262144
    expect(maxCacheByteCapacity(h)).toBe(2 ** 12 * 64);
  });
});
