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
  pageSize: 4096,
  pageTableLevels: 1,
  pageWalkStart: "RAM",
  entries: 64,
  associativity: 4,
  replacement: "LRU",
};

describe("computeLayout — base chain", () => {
  it("without VM: CPU, REG, RAM only — RAM is terminal so it has no Size line and isn't clickable", () => {
    const layout = computeLayout([], defaultRamConfig(), defaultDiskConfig(), tlb, false);
    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "reg", "ram"]);

    const cpu = layout.boxes.find((b) => b.id === "cpu")!;
    expect(cpu.label).toBe("CPU");
    expect(cpu.subLabels).toEqual([]);
    expect(cpu.clickable).toBe(false);

    const reg = layout.boxes.find((b) => b.id === "reg")!;
    expect(reg.label).toBe("REG");
    expect(reg.subLabels).toEqual([]);
    expect(reg.clickable).toBe(true);
    expect(reg.targetTabId).toBeNull();

    const ram = layout.boxes.find((b) => b.id === "ram")!;
    expect(ram.subLabels).toEqual(["Latency: 100 cy"]);
    expect(ram.clickable).toBe(false);
  });

  it("with VM: adds TLB (branch, below CPU) and DISK (terminal); RAM shows Size and is clickable", () => {
    const layout = computeLayout([], defaultRamConfig(), defaultDiskConfig(), tlb, true);
    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "reg", "ram", "disk", "tlb"]);

    const ram = layout.boxes.find((b) => b.id === "ram")!;
    expect(ram.subLabels).toEqual(["Size: 1 KB", "Latency: 100 cy"]);
    expect(ram.clickable).toBe(true);
    expect(ram.targetTabId).toBe("ram");

    const disk = layout.boxes.find((b) => b.id === "disk")!;
    expect(disk.label).toBe("DISK");
    expect(disk.subLabels).toEqual(["Latency: 10K cy"]);
    expect(disk.clickable).toBe(false);

    const tlbBox = layout.boxes.find((b) => b.id === "tlb")!;
    expect(tlbBox.label).toBe("TLB");
    expect(tlbBox.subLabels).toEqual(["Entries: 64"]);
    expect(tlbBox.clickable).toBe(true);
    expect(tlbBox.targetTabId).toBe("tlb");

    // TLB sits below CPU, not in the main horizontal chain.
    const cpu = layout.boxes.find((b) => b.id === "cpu")!;
    expect(tlbBox.y).toBeGreaterThan(cpu.y + cpu.height);

    // One connector links CPU to TLB, one links each pair in the main chain
    // (cpu-reg, reg-ram, ram-disk) = 4 total.
    expect(layout.connectors).toHaveLength(4);
    expect(layout.connectors.some((c) => c.id === "cpu-tlb")).toBe(true);
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
  it("inserts one box per unified level, in order, between REG and RAM", () => {
    const hierarchy = [unifiedLevel("l1", 10, 64, 1), unifiedLevel("l2", 13, 64, 10)];
    const layout = computeLayout(hierarchy, defaultRamConfig(), defaultDiskConfig(), tlb, false);

    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "reg", "l1", "l2", "ram"]);

    const l1 = layout.boxes.find((b) => b.id === "l1")!;
    expect(l1.label).toBe("L1");
    // 2^10 entries * 64 bytes/entry = 65536 bytes = 64 KB
    expect(l1.subLabels).toEqual(["Size: 64 KB", "Latency: 1 cy"]);
    expect(l1.clickable).toBe(true);
    expect(l1.targetTabId).toBe("l1");

    const l2 = layout.boxes.find((b) => b.id === "l2")!;
    expect(l2.label).toBe("L2");
    // 2^13 entries * 64 bytes/entry = 524288 bytes = 512 KB
    expect(l2.subLabels).toEqual(["Size: 512 KB", "Latency: 10 cy"]);

    // Boxes are laid out left-to-right in slot order.
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
    const layout = computeLayout(hierarchy, defaultRamConfig(), defaultDiskConfig(), tlb, false);

    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "reg", "l1-i", "l1-d", "ram"]);

    const li = layout.boxes.find((b) => b.id === "l1-i")!;
    const ld = layout.boxes.find((b) => b.id === "l1-d")!;
    expect(li.label).toBe("L1i");
    expect(ld.label).toBe("L1d");
    expect(li.clickable).toBe(true);
    expect(ld.clickable).toBe(true);
    expect(li.targetTabId).toBe("l1-i");
    expect(ld.targetTabId).toBe("l1-d");

    // Stacked: same x, i above d.
    expect(li.x).toBe(ld.x);
    expect(li.y).toBeLessThan(ld.y);

    // The connector arriving from REG forks into two paths (one per box);
    // the connector leaving toward RAM merges back into two paths as well.
    const forkPaths = layout.connectors.filter((c) => c.id.startsWith("link-1-"));
    const mergePaths = layout.connectors.filter((c) => c.id.startsWith("link-2-"));
    expect(forkPaths).toHaveLength(2);
    expect(mergePaths).toHaveLength(2);
  });
});
