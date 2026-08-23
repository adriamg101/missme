import { describe, it, expect } from "vitest";
import { byteSizeLabel, computeLayout } from "./diagram";
import { defaultRamConfig, defaultDiskConfig } from "./model";
import type { TlbConfig } from "./model";

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
    expect(ram.subLabels).toEqual(["Latency: 100 cycles"]);
    expect(ram.clickable).toBe(false);
  });

  it("with VM: adds TLB (branch, below CPU) and DISK (terminal); RAM shows Size and is clickable", () => {
    const layout = computeLayout([], defaultRamConfig(), defaultDiskConfig(), tlb, true);
    expect(layout.boxes.map((b) => b.id)).toEqual(["cpu", "reg", "ram", "disk", "tlb"]);

    const ram = layout.boxes.find((b) => b.id === "ram")!;
    expect(ram.subLabels).toEqual(["Size: 1 KB", "Latency: 100 cycles"]);
    expect(ram.clickable).toBe(true);
    expect(ram.targetTabId).toBe("ram");

    const disk = layout.boxes.find((b) => b.id === "disk")!;
    expect(disk.label).toBe("DISK");
    expect(disk.subLabels).toEqual(["Latency: 10000 cycles"]);
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
