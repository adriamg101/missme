import { describe, it, expect } from "vitest";
import type { Hierarchy, Level } from "./model";
import { defaultCacheConfig } from "./model";
import { createLevel, removeLevel, toHarvard, toVonNeumann, inheritedConfigFor, moveMarker, clampToIsa } from "./operations";

function unifiedLevel(id: string, exp: number, blockSize = 64): Level {
  return {
    id,
    harvard: false,
    unified: { exp, config: { ...defaultCacheConfig(), blockSize } },
    inclusion: "inclusive",
    busWidth: blockSize,
  };
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

describe("createLevel", () => {
  it("creates L1 on an empty hierarchy", () => {
    const result = createLevel([], 10, "new");
    expect(result).not.toBeNull();
    expect(result!.map((l) => l.id)).toEqual(["new"]);
    expect(result![0].unified!.exp).toBe(10);
  });

  it("inserts between two levels and renames follow position, not id", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 20)];
    const result = createLevel(h, 12, "new")!;
    expect(result.map((l) => l.id)).toEqual(["a", "new", "b"]);
  });

  it("inserts before the first level", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 20)];
    const result = createLevel(h, 1, "new")!;
    expect(result.map((l) => l.id)).toEqual(["new", "a", "b"]);
  });

  it("refuses to insert when there is no gap between adjacent levels", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 6)];
    expect(createLevel(h, 5, "new")).toBeNull();
    expect(createLevel(h, 6, "new")).toBeNull();
  });

  it("refuses to insert inside a harvard level's span", () => {
    const h: Hierarchy = [harvardLevel("a", 10, 20)];
    expect(createLevel(h, 15, "new")).toBeNull();
  });

  it("new levels are born unified and inherit the config of the nearest lower level (its data side if harvard)", () => {
    const h: Hierarchy = [harvardLevel("a", 5, 8)];
    (h[0].d as any).config.blockSize = 128;
    const result = createLevel(h, 15, "new")!;
    const created = result.find((l) => l.id === "new")!;
    expect(created.harvard).toBe(false);
    expect(created.unified!.config.blockSize).toBe(128);
  });

  it("the first level inherits the default config (no lower level to copy from)", () => {
    const config = inheritedConfigFor([], 0);
    expect(config).toEqual(defaultCacheConfig());
  });
});

describe("removeLevel", () => {
  it("removes a unified level entirely", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 20)];
    expect(removeLevel(h, "a").map((l) => l.id)).toEqual(["b"]);
  });

  it("removes both sides of a harvard level in one call", () => {
    const h: Hierarchy = [harvardLevel("a", 5, 8), unifiedLevel("b", 20)];
    expect(removeLevel(h, "a").map((l) => l.id)).toEqual(["b"]);
  });
});

describe("toHarvard", () => {
  it("splits a unified level into i/d at the same position, both inheriting its config", () => {
    const h: Hierarchy = [unifiedLevel("a", 10, 128)];
    const result = toHarvard(h, "a");
    const level = result[0];
    expect(level.harvard).toBe(true);
    expect(level.i!.exp).toBe(10);
    expect(level.d!.exp).toBe(10);
    expect(level.i!.config.blockSize).toBe(128);
    expect(level.d!.config.blockSize).toBe(128);
    expect(level.unified).toBeUndefined();
  });

  it("is a no-op on an already-harvard level", () => {
    const h: Hierarchy = [harvardLevel("a", 5, 8)];
    expect(toHarvard(h, "a")).toEqual(h);
  });
});

describe("toVonNeumann", () => {
  it("merges, keeping the instruction side's position and config entirely", () => {
    const h: Hierarchy = [harvardLevel("a", 5, 8)];
    (h[0].i as any).config.blockSize = 256;
    const result = toVonNeumann(h, "a");
    expect(result.ok).toBe(true);
    if (result.ok) {
      const level = result.hierarchy.find((l: Level) => l.id === "a")!;
      expect(level.harvard).toBe(false);
      expect(level.unified!.exp).toBe(5);
      expect(level.unified!.config.blockSize).toBe(256);
    }
  });

  it("rejects the merge when moving the data marker would break the data chain, and reports the conflicting index", () => {
    // L1 harvard: i=5, d=8. L2 unified at 6.
    // Merging L1 keeps i=5 as the unified position, but the data chain was [8, 6] -> becomes [5, 6] (fine)...
    // so instead test the reverse: L2 harvard i=15,d=9 with L3 unified at 10 -> merging L2 sets d to 15, breaking [15,10].
    const h: Hierarchy = [unifiedLevel("l1", 2), harvardLevel("l2", 15, 9), unifiedLevel("l3", 10)];
    const result = toVonNeumann(h, "l2");
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.conflictIndex).toBe(2);
    }
  });

  it("is a no-op on an already-unified level", () => {
    const h: Hierarchy = [unifiedLevel("a", 5)];
    const result = toVonNeumann(h, "a");
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.hierarchy).toEqual(h);
    }
  });
});

describe("moveMarker", () => {
  it("clamps against the previous and next marker in the same chain", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 10), unifiedLevel("c", 20)];
    const movedTooFarRight = moveMarker(h, "b", "unified", 25, "RV32I");
    expect(movedTooFarRight.find((l) => l.id === "b")!.unified!.exp).toBe(19); // stuck just below c(20)

    const movedTooFarLeft = moveMarker(h, "b", "unified", 0, "RV32I");
    expect(movedTooFarLeft.find((l) => l.id === "b")!.unified!.exp).toBe(6); // stuck just above a(5)
  });

  it("clamps against the slider extremes when there is no neighbor", () => {
    const h: Hierarchy = [unifiedLevel("a", 5)];
    const movedLeft = moveMarker(h, "a", "unified", -10, "RV32I");
    expect(movedLeft.find((l) => l.id === "a")!.unified!.exp).toBe(1); // MIN_POSITION

    const movedRight = moveMarker(h, "a", "unified", 999, "RV32I");
    expect(movedRight.find((l) => l.id === "a")!.unified!.exp).toBe(31); // RV32I max position
  });

  it("moves a harvard level's i and d markers independently, each against its own chain", () => {
    const h: Hierarchy = [harvardLevel("a", 5, 25)];
    const movedI = moveMarker(h, "a", "i", 100, "RV32I");
    expect(movedI.find((l) => l.id === "a")!.i!.exp).toBe(31);
    expect(movedI.find((l) => l.id === "a")!.d!.exp).toBe(25); // d untouched
  });
});

describe("clampToIsa", () => {
  it("clamps out-of-range markers down to the new max and flags no confirmation when order is preserved", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 40)];
    const result = clampToIsa(h, "RV32I");
    expect(result.requiresConfirmation).toBe(false);
    expect(result.hierarchy.find((l) => l.id === "b")!.unified!.exp).toBe(31);
  });

  it("flags requiresConfirmation when clamping would collide two levels in the same chain", () => {
    const h: Hierarchy = [unifiedLevel("a", 31), unifiedLevel("b", 40)];
    const result = clampToIsa(h, "RV32I");
    expect(result.requiresConfirmation).toBe(true);
  });

  it("leaves an already-valid hierarchy untouched", () => {
    const h: Hierarchy = [unifiedLevel("a", 5), unifiedLevel("b", 10)];
    const result = clampToIsa(h, "RV32I");
    expect(result.requiresConfirmation).toBe(false);
    expect(result.hierarchy).toEqual(h);
  });
});
