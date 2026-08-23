import type { DiskConfig, Hierarchy, RamConfig, TlbConfig } from "./model";
import { chainLabel, levelName } from "./derive";

const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

// bytes is always an exact power of two here (entries and blockSize are both
// powers of two), so log2(bytes) is always a whole number.
export function byteSizeLabel(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const unitIndex = Math.min(Math.floor(Math.log2(bytes) / 10), SIZE_UNITS.length - 1);
  const value = bytes / 2 ** (unitIndex * 10);
  return `${value} ${SIZE_UNITS[unitIndex]}`;
}

export type DiagramBoxKind = "cpu" | "reg" | "cache" | "ram" | "disk" | "tlb";

export interface DiagramBox {
  id: string;
  kind: DiagramBoxKind;
  label: string;
  subLabels: string[];
  x: number;
  y: number;
  width: number;
  height: number;
  fill: string;
  textFill: string;
  clickable: boolean;
  targetTabId: string | null;
}

export interface DiagramConnector {
  id: string;
  path: string;
}

export interface DiagramLayout {
  width: number;
  height: number;
  boxes: DiagramBox[];
  connectors: DiagramConnector[];
}

interface SlotBoxSpec {
  id: string;
  kind: DiagramBoxKind;
  label: string;
  subLabels: string[];
  fill: string;
  textFill: string;
  clickable: boolean;
  targetTabId: string | null;
}

// One slot is a single box (CPU/REG/unified cache/RAM/DISK) or two stacked
// boxes (a Harvard-split cache level: i on top, d on bottom).
type Slot = SlotBoxSpec[];

const BOX_WIDTH = 90;
const BOX_HEIGHT = 60;
const SLOT_GAP = 60;
const CENTER_Y = 120;
const HARVARD_OFFSET = 45;
const TLB_WIDTH = 70;
const TLB_HEIGHT = 50;
const TLB_GAP = 30;

export function computeLayout(
  hierarchy: Hierarchy,
  ram: RamConfig,
  disk: DiskConfig,
  tlb: TlbConfig,
  vmEnabled: boolean,
): DiagramLayout {
  const slots: Slot[] = [];

  slots.push([
    { id: "cpu", kind: "cpu", label: "CPU", subLabels: [], fill: "var(--background)", textFill: "var(--text)", clickable: false, targetTabId: null },
  ]);
  slots.push([
    { id: "reg", kind: "reg", label: "REG", subLabels: [], fill: "var(--background)", textFill: "var(--text)", clickable: true, targetTabId: null },
  ]);

  hierarchy.forEach((level, index) => {
    if (level.harvard) {
      const iBytes = 2 ** level.i!.exp * level.i!.config.blockSize;
      const dBytes = 2 ** level.d!.exp * level.d!.config.blockSize;
      slots.push([
        {
          id: `${level.id}-i`,
          kind: "cache",
          label: chainLabel(hierarchy, index, "i"),
          subLabels: [`Size: ${byteSizeLabel(iBytes)}`, `Latency: ${level.i!.config.hitTime} cycles`],
          fill: "var(--accent)",
          textFill: "var(--background)",
          clickable: true,
          targetTabId: `${level.id}-i`,
        },
        {
          id: `${level.id}-d`,
          kind: "cache",
          label: chainLabel(hierarchy, index, "d"),
          subLabels: [`Size: ${byteSizeLabel(dBytes)}`, `Latency: ${level.d!.config.hitTime} cycles`],
          fill: "var(--accent)",
          textFill: "var(--background)",
          clickable: true,
          targetTabId: `${level.id}-d`,
        },
      ]);
    } else {
      const bytes = 2 ** level.unified!.exp * level.unified!.config.blockSize;
      slots.push([
        {
          id: level.id,
          kind: "cache",
          label: levelName(hierarchy, index),
          subLabels: [`Size: ${byteSizeLabel(bytes)}`, `Latency: ${level.unified!.config.hitTime} cycles`],
          fill: "var(--accent)",
          textFill: "var(--background)",
          clickable: true,
          targetTabId: level.id,
        },
      ]);
    }
  });

  const ramSubLabels = vmEnabled
    ? [`Size: ${byteSizeLabel(ram.size)}`, `Latency: ${ram.latency} cycles`]
    : [`Latency: ${ram.latency} cycles`];
  slots.push([
    { id: "ram", kind: "ram", label: "RAM", subLabels: ramSubLabels, fill: "var(--primary)", textFill: "var(--background)", clickable: true, targetTabId: "ram" },
  ]);

  if (vmEnabled) {
    slots.push([
      {
        id: "disk",
        kind: "disk",
        label: "DISK",
        subLabels: [`Latency: ${disk.latency} cycles`],
        fill: "var(--secondary)",
        textFill: "var(--background)",
        clickable: true,
        targetTabId: "disk",
      },
    ]);
  }

  // First (CPU) and last (terminal) slot in the main chain are never clickable.
  slots[0][0].clickable = false;
  slots[slots.length - 1].forEach((box) => (box.clickable = false));

  const boxes: DiagramBox[] = [];
  const slotX: number[] = [];
  let cursorX = 2;
  slots.forEach((slot) => {
    slotX.push(cursorX);
    if (slot.length === 1) {
      boxes.push({ ...slot[0], x: cursorX, y: CENTER_Y - BOX_HEIGHT / 2, width: BOX_WIDTH, height: BOX_HEIGHT });
    } else {
      boxes.push({ ...slot[0], x: cursorX, y: CENTER_Y - HARVARD_OFFSET - BOX_HEIGHT / 2, width: BOX_WIDTH, height: BOX_HEIGHT });
      boxes.push({ ...slot[1], x: cursorX, y: CENTER_Y + HARVARD_OFFSET - BOX_HEIGHT / 2, width: BOX_WIDTH, height: BOX_HEIGHT });
    }
    cursorX += BOX_WIDTH + SLOT_GAP;
  });

  const connectors: DiagramConnector[] = [];
  let tlbBox: DiagramBox | null = null;
  if (vmEnabled) {
    const cpuBox = boxes[0];
    const tlbX = cpuBox.x + (BOX_WIDTH - TLB_WIDTH) / 2;
    const tlbY = cpuBox.y + BOX_HEIGHT + TLB_GAP;
    tlbBox = {
      id: "tlb",
      kind: "tlb",
      label: "TLB",
      subLabels: [`Entries: ${tlb.entries}`],
      x: tlbX,
      y: tlbY,
      width: TLB_WIDTH,
      height: TLB_HEIGHT,
      fill: "var(--background)",
      textFill: "var(--text)",
      clickable: true,
      targetTabId: "tlb",
    };
    boxes.push(tlbBox);
    connectors.push({
      id: "cpu-tlb",
      path: `M ${cpuBox.x + BOX_WIDTH / 2} ${cpuBox.y + BOX_HEIGHT} L ${tlbBox.x + TLB_WIDTH / 2} ${tlbBox.y}`,
    });
  }

  for (let i = 0; i < slots.length - 1; i++) {
    const a = slots[i];
    const b = slots[i + 1];
    const aRightX = slotX[i] + BOX_WIDTH;
    const bLeftX = slotX[i + 1];
    const topY = CENTER_Y - HARVARD_OFFSET;
    const bottomY = CENTER_Y + HARVARD_OFFSET;

    if (a.length === 1 && b.length === 1) {
      connectors.push({ id: `link-${i}`, path: `M ${aRightX} ${CENTER_Y} L ${bLeftX} ${CENTER_Y}` });
    } else if (a.length === 1 && b.length === 2) {
      connectors.push({ id: `link-${i}-top`, path: `M ${aRightX} ${CENTER_Y} L ${bLeftX} ${topY}` });
      connectors.push({ id: `link-${i}-bottom`, path: `M ${aRightX} ${CENTER_Y} L ${bLeftX} ${bottomY}` });
    } else if (a.length === 2 && b.length === 1) {
      connectors.push({ id: `link-${i}-top`, path: `M ${aRightX} ${topY} L ${bLeftX} ${CENTER_Y}` });
      connectors.push({ id: `link-${i}-bottom`, path: `M ${aRightX} ${bottomY} L ${bLeftX} ${CENTER_Y}` });
    } else {
      const midX = (aRightX + bLeftX) / 2;
      connectors.push({ id: `link-${i}-in-top`, path: `M ${aRightX} ${topY} L ${midX} ${CENTER_Y}` });
      connectors.push({ id: `link-${i}-in-bottom`, path: `M ${aRightX} ${bottomY} L ${midX} ${CENTER_Y}` });
      connectors.push({ id: `link-${i}-out-top`, path: `M ${midX} ${CENTER_Y} L ${bLeftX} ${topY}` });
      connectors.push({ id: `link-${i}-out-bottom`, path: `M ${midX} ${CENTER_Y} L ${bLeftX} ${bottomY}` });
    }
  }

  const mainAxisBottom = CENTER_Y + HARVARD_OFFSET + BOX_HEIGHT / 2;
  const tlbBottom = tlbBox ? tlbBox.y + TLB_HEIGHT : 0;
  const width = cursorX - SLOT_GAP + 20;
  const height = Math.max(mainAxisBottom, tlbBottom) + 20;

  return { width, height, boxes, connectors };
}
