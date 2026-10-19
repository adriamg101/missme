import { describe, it, expect } from "vitest";
import { byteSizeLabel, cycleLabel, cacheHue, computeLayout } from "./diagram";
import { defaultRamConfig, defaultDiskConfig } from "./model";
import type { TlbConfig, Hierarchy } from "./model";

describe("byteSizeLabel", () => {
  it("labels bytes below 1024 with no unit scaling", () => {
    expect(byteSizeLabel(8)).toBe("8 B");
  });

  it("scales to KB at 1024 bytes", () => {
    expect(byteSizeLabel(1024)).toBe("1 KB");
  });

  it("scales to MB", () => {
    expect(byteSizeLabel(32 * 1024 * 1024)).toBe("32 MB");
  });

  it("scales to GB", () => {
    expect(byteSizeLabel(4 * 1024 * 1024 * 1024)).toBe("4 GB");
  });

  it("returns 0 B for zero input", () => {
    expect(byteSizeLabel(0)).toBe("0 B");
  });

  it("returns 0 B for non-finite input (e.g. a legacy config missing the field)", () => {
    expect(byteSizeLabel(NaN)).toBe("0 B");
    expect(byteSizeLabel(undefined as unknown as number)).toBe("0 B");
  });
});

describe("cycleLabel", () => {
  it("labels counts below 1000 with no unit letter", () => {
    expect(cycleLabel(1)).toBe("1 cy");
    expect(cycleLabel(100)).toBe("100 cy");
  });

  it("scales to K at 1000", () => {
    expect(cycleLabel(1000)).toBe("1K cy");
    expect(cycleLabel(10000)).toBe("10K cy");
  });

  it("scales to M and B, rounding to one decimal place", () => {
    expect(cycleLabel(2_500_000)).toBe("2.5M cy");
    expect(cycleLabel(3_000_000_000)).toBe("3B cy");
  });

  it("returns 0 cy for non-finite or negative input", () => {
    expect(cycleLabel(NaN)).toBe("0 cy");
    expect(cycleLabel(-5)).toBe("0 cy");
  });
});

describe("cacheHue", () => {
  it("returns the warm anchor for a single level", () => {
    expect(cacheHue(0, 1)).toBe(45);
  });

  it("spans warm -> mid -> cool across the first and last of several levels", () => {
    expect(cacheHue(0, 3)).toBe(45);
    expect(cacheHue(1, 3)).toBe(130);
    expect(cacheHue(2, 3)).toBe(215);
  });
});

const tlb: TlbConfig = {
  pageWalkStart: "RAM",
  entries: 64,
  associativity: 4,
  replacement: "LRU",
  latency: 4,
};

describe("computeLayout — base chain", () => {
  it("without VM: CPU, RAM, REG only — RAM is terminal, has no Size line, but stays clickable", () => {
    const layout = computeLayout([], defaultRamConfig(), 4096, defaultDiskConfig(), tlb, false);
    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "ram", "reg"]);

    const cpu = layout.boxes.find((b) => b.id === "cpu")!;
    expect(cpu.label).toBe("CPU");
    expect(cpu.subLabels).toEqual([]);
    expect(cpu.clickable).toBe(false);

    const ram = layout.boxes.find((b) => b.id === "ram")!;
    expect(ram.subLabels).toEqual(["Latency: 100 cy"]);
    expect(ram.clickable).toBe(true);

    // REG sits inside the CPU box instead of being part of the main chain,
    // so it stays fully inside the CPU box's rectangle.
    const reg = layout.boxes.find((b) => b.id === "reg")!;
    expect(reg.label).toBe("REG");
    expect(reg.subLabels).toEqual([]);
    expect(reg.clickable).toBe(true);
    expect(reg.targetTabId).toBeNull();
    expect(reg.x).toBeGreaterThanOrEqual(cpu.x);
    expect(reg.x + reg.width).toBeLessThanOrEqual(cpu.x + cpu.width);
    expect(reg.y).toBeGreaterThanOrEqual(cpu.y);
    expect(reg.y + reg.height).toBeLessThanOrEqual(cpu.y + cpu.height);

    // There's no separate line connecting CPU and REG, since REG doesn't have its own wire.
    expect(layout.connectors.some((c) => c.id.includes("reg"))).toBe(false);
  });

  it("under VM with no Harvard-split cache, RAM's taller box still fits inside the diagram (never a negative y)", () => {
    // RAM's box is much taller than the rest under VM. With no split cache
    // to push the shared vertical center down, RAM's top edge used to land
    // above y=0 and get clipped by the diagram's own edge.
    const layout = computeLayout([], defaultRamConfig(), 4096, defaultDiskConfig(), tlb, true);
    const ram = layout.boxes.find((b) => b.id === "ram")!;
    expect(ram.y).toBeGreaterThanOrEqual(0);
  });

  it("with VM: adds TLB (branch, below CPU) and DISK (terminal); RAM shows Size and splits into PT/Frames", () => {
    const layout = computeLayout([], defaultRamConfig(), 4096, defaultDiskConfig(), tlb, true);
    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "ram", "disk", "tlb", "ram-pt", "ram-frames", "reg"]);

    const ram = layout.boxes.find((b) => b.id === "ram")!;
    // The frame count now shows on the Frames sub-box instead, see below.
    expect(ram.subLabels).toEqual(["Latency: 100 cy"]);
    // Once VM is enabled, RAM's own box stops being clickable in favor of
    // its PT/Frames sub-blocks, drawn inside it.
    expect(ram.clickable).toBe(false);
    expect(ram.targetTabId).toBe("ram");

    // defaultRamConfig() sets frames to 4, and this test's page size is
    // 4096, so the frame pool's real byte capacity is 4 * 4096 = 16 KB.
    const framesBox = layout.boxes.find((b) => b.id === "ram-frames")!;
    expect(framesBox.subLabels).toEqual(["Size: 16 KB"]);
    const ptBox = layout.boxes.find((b) => b.id === "ram-pt")!;
    expect(ptBox.subLabels).toEqual([]);

    const disk = layout.boxes.find((b) => b.id === "disk")!;
    expect(disk.label).toBe("DISK");
    expect(disk.subLabels).toEqual(["Latency: 10K cy"]);
    // DISK has its own inspection view now (TT4.6), so unlike a generic
    // terminal box it stays clickable even though it's last in the chain.
    expect(disk.clickable).toBe(true);

    const tlbBox = layout.boxes.find((b) => b.id === "tlb")!;
    expect(tlbBox.label).toBe("TLB");
    expect(tlbBox.subLabels).toEqual(["Entries: 64"]);
    expect(tlbBox.clickable).toBe(true);
    expect(tlbBox.targetTabId).toBe("tlb");

    // The TLB box sits below CPU, not in the main left-to-right chain.
    const cpu = layout.boxes.find((b) => b.id === "cpu")!;
    expect(tlbBox.y).toBeGreaterThan(cpu.y + cpu.height);

    // One line connects CPU to TLB, one line connects each pair in the main
    // chain (cpu to ram, ram to disk), and one line routes from TLB down to
    // the page-walk target (RAM by default), for 4 total.
    expect(layout.connectors).toHaveLength(4);
    expect(layout.connectors.some((c) => c.id === "cpu-tlb")).toBe(true);
    expect(layout.connectors.some((c) => c.id === "pagewalk-ram")).toBe(true);
  });

  it("routes TLB -> page-walk target through a shared lane below the main chain, all at right angles", () => {
    const hierarchy = [unifiedLevel("l1", 10, 64, 1), unifiedLevel("l2", 13, 64, 10)];
    const customTlb: TlbConfig = { ...tlb, pageWalkStart: "l2" };
    const layout = computeLayout(hierarchy, defaultRamConfig(), 4096, defaultDiskConfig(), customTlb, true);

    const pageWalkConnector = layout.connectors.find((c) => c.id === "pagewalk-l2")!;
    expect(pageWalkConnector).toBeTruthy();
    // Each point in the path shares either its x or its y with the point
    // before it, meaning there are no diagonal segments.
    const points = pageWalkConnector.path
      .split(/[ML]/)
      .map((s) => s.trim())
      .filter(Boolean)
      .map((pair) => pair.split(" ").map(Number));
    for (let i = 1; i < points.length; i++) {
      const [x1, y1] = points[i - 1];
      const [x2, y2] = points[i];
      expect(x1 === x2 || y1 === y2).toBe(true);
    }

    // No line anywhere in the diagram should be diagonal.
    layout.connectors.forEach((connector) => {
      const pts = connector.path
        .split(/[ML]/)
        .map((s) => s.trim())
        .filter(Boolean)
        .map((pair) => pair.split(" ").map(Number));
      for (let i = 1; i < pts.length; i++) {
        const [x1, y1] = pts[i - 1];
        const [x2, y2] = pts[i];
        expect(x1 === x2 || y1 === y2).toBe(true);
      }
    });
  });

  it("fans a page-walk target out to both boxes when it's a Harvard-split level", () => {
    const hierarchy = [harvardLevel("l1", 10, 11, 64)];
    const customTlb: TlbConfig = { ...tlb, pageWalkStart: "l1" };
    const layout = computeLayout(hierarchy, defaultRamConfig(), 4096, defaultDiskConfig(), customTlb, true);

    expect(layout.connectors.some((c) => c.id === "pagewalk-l1-i")).toBe(true);
    expect(layout.connectors.some((c) => c.id === "pagewalk-l1-d")).toBe(true);
  });

  it("splits the RAM box into PT and Frames sub-blocks when VM is enabled, drawn on top of a non-clickable outer RAM box", () => {
    const layout = computeLayout([], defaultRamConfig(), 4096, defaultDiskConfig(), tlb, true);
    const ramBox = layout.boxes.find((b) => b.id === "ram")!;
    const ptBox = layout.boxes.find((b) => b.id === "ram-pt")!;
    const framesBox = layout.boxes.find((b) => b.id === "ram-frames")!;

    expect(ramBox.clickable).toBe(false);
    expect(ptBox.clickable).toBe(true);
    expect(framesBox.clickable).toBe(true);
    expect(ptBox.kind).toBe("pt");
    expect(framesBox.kind).toBe("frames");
    // Both sub-blocks must sit inside the outer RAM box's rect.
    expect(ptBox.x).toBeGreaterThanOrEqual(ramBox.x);
    expect(ptBox.x + ptBox.width).toBeLessThanOrEqual(ramBox.x + ramBox.width);
    expect(framesBox.x).toBeGreaterThanOrEqual(ramBox.x);
    expect(framesBox.x + framesBox.width).toBeLessThanOrEqual(ramBox.x + ramBox.width);
    // They must be drawn after the outer RAM box (so they paint on top).
    const ramDrawIndex = layout.boxes.indexOf(ramBox);
    expect(layout.boxes.indexOf(ptBox)).toBeGreaterThan(ramDrawIndex);
    expect(layout.boxes.indexOf(framesBox)).toBeGreaterThan(ramDrawIndex);
  });

  it("RAM stays a single clickable box with no PT/Frames sub-blocks when VM is disabled", () => {
    const layout = computeLayout([], defaultRamConfig(), 4096, defaultDiskConfig(), tlb, false);
    const ramBox = layout.boxes.find((b) => b.id === "ram")!;
    expect(ramBox.clickable).toBe(true);
    expect(layout.boxes.find((b) => b.id === "ram-pt")).toBeUndefined();
    expect(layout.boxes.find((b) => b.id === "ram-frames")).toBeUndefined();
  });
});

function unifiedLevel(id: string, exp: number, blockSize: number, hitTime: number): Hierarchy[number] {
  return {
    id,
    harvard: false,
    unified: {
      exp,
      config: { blockSize, associativity: 1, replacement: "LRU", writePolicy: "write-back", writeAllocate: true, hitTime, dirtyPercent: 0 },
    },
    inclusion: "inclusive",
    busWidth: 32,
  };
}

describe("computeLayout — unified cache levels", () => {
  it("inserts one box per unified level, in order, between CPU and RAM", () => {
    const hierarchy = [unifiedLevel("l1", 10, 64, 1), unifiedLevel("l2", 13, 64, 10)];
    const layout = computeLayout(hierarchy, defaultRamConfig(), 4096, defaultDiskConfig(), tlb, false);

    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "l1", "l2", "ram", "reg"]);

    const l1 = layout.boxes.find((b) => b.id === "l1")!;
    expect(l1.label).toBe("L1");
    // 2^10 entries times 64 bytes each = 65536 bytes = 64 KB
    expect(l1.subLabels).toEqual(["Size: 64 KB", "Latency: 1 cy"]);
    expect(l1.clickable).toBe(true);
    expect(l1.targetTabId).toBe("l1");

    const l2 = layout.boxes.find((b) => b.id === "l2")!;
    expect(l2.label).toBe("L2");
    // 2^13 entries times 64 bytes each = 524288 bytes = 512 KB
    expect(l2.subLabels).toEqual(["Size: 512 KB", "Latency: 10 cy"]);

    // Boxes are placed left to right in the same order as their slots.
    expect(l1.x).toBeLessThan(l2.x);
    const ram = layout.boxes.find((b) => b.id === "ram")!;
    expect(l2.x).toBeLessThan(ram.x);
  });
});

function harvardLevel(id: string, iExp: number, dExp: number, blockSize: number): Hierarchy[number] {
  const config = { blockSize, associativity: 1, replacement: "LRU" as const, writePolicy: "write-back" as const, writeAllocate: true, hitTime: 1, dirtyPercent: 0 };
  return {
    id,
    harvard: true,
    i: { exp: iExp, config },
    d: { exp: dExp, config },
    inclusion: "inclusive",
    busWidth: 32,
  };
}

describe("computeLayout — Harvard-split cache levels", () => {
  it("produces two stacked boxes (i above, d below) instead of one, both clickable", () => {
    const hierarchy = [harvardLevel("l1", 10, 11, 64)];
    const layout = computeLayout(hierarchy, defaultRamConfig(), 4096, defaultDiskConfig(), tlb, false);

    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "l1-i", "l1-d", "ram", "reg"]);

    const li = layout.boxes.find((b) => b.id === "l1-i")!;
    const ld = layout.boxes.find((b) => b.id === "l1-d")!;
    expect(li.label).toBe("L1i");
    expect(ld.label).toBe("L1d");
    expect(li.clickable).toBe(true);
    expect(ld.clickable).toBe(true);
    expect(li.targetTabId).toBe("l1-i");
    expect(ld.targetTabId).toBe("l1-d");

    // Stacked: same x position, instruction box above the data box.
    expect(li.x).toBe(ld.x);
    expect(li.y).toBeLessThan(ld.y);

    // The line coming from CPU splits into two paths, one for each box, and
    // the line going toward RAM merges back into two paths as well.
    const forkPaths = layout.connectors.filter((c) => c.id.startsWith("link-0-"));
    const mergePaths = layout.connectors.filter((c) => c.id.startsWith("link-1-"));
    expect(forkPaths).toHaveLength(2);
    expect(mergePaths).toHaveLength(2);
  });
});
