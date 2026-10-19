import type { DiskConfig, Hierarchy, RamConfig, TlbConfig } from "./model";
import { chainLabel, levelName, sizeLabel } from "./derive";

const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

// bytes is always an exact power of two here, since entries and block size
// both are, so log2(bytes) is always a whole number.
export function byteSizeLabel(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const unitIndex = Math.min(Math.floor(Math.log2(bytes) / 10), SIZE_UNITS.length - 1);
  const value = bytes / 2 ** (unitIndex * 10);
  return `${value} ${SIZE_UNITS[unitIndex]}`;
}

const CYCLE_UNITS = ["", "K", "M", "B"];

// Cycle counts are plain decimal numbers, not powers of two like byte
// counts, so this rounds to one decimal place instead of dividing exactly.
export function cycleLabel(cycles: number): string {
  if (!Number.isFinite(cycles) || cycles < 0) return "0 cy";
  let value = cycles;
  let unitIndex = 0;
  while (value >= 1000 && unitIndex < CYCLE_UNITS.length - 1) {
    value /= 1000;
    unitIndex++;
  }
  const rounded = Math.round(value * 10) / 10;
  return `${rounded}${CYCLE_UNITS[unitIndex]} cy`;
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

// Cache levels get a color gradient, warm near the CPU and cool at the
// deepest level, so any number of levels stay visually distinct.
const CACHE_HUE_WARM = 45;
const CACHE_HUE_MID = 130;
const CACHE_HUE_COOL = 215;

export function cacheHue(levelIndex: number, totalLevels: number): number {
  if (totalLevels <= 1) return CACHE_HUE_WARM;
  const t = levelIndex / (totalLevels - 1);
  if (t < 0.5) return lerp(CACHE_HUE_WARM, CACHE_HUE_MID, t * 2);
  return lerp(CACHE_HUE_MID, CACHE_HUE_COOL, (t - 0.5) * 2);
}

// Fixed colors per box kind, from the design spec. Cache is not listed here
// since its color is worked out per level by cacheHue instead of being fixed.
export const TYPE_CHROMA = 0.07;
export const CACHE_CHROMA = 0.09;

// A Harvard level's instruction side is drawn slightly lighter than its
// data side, so the two stay visually distinct despite sharing a hue.
export const HARVARD_INSTRUCTION_LIGHTNESS_DELTA = 0.06;

export const HUE_BY_KIND: Record<Exclude<DiagramBoxKind, "cache">, number> = {
  cpu: 15,
  reg: 95,
  tlb: 330,
  ram: 255,
  disk: 290,
  pt: 285,
  frames: 215,
};

export type DiagramBoxKind = "cpu" | "reg" | "cache" | "ram" | "disk" | "tlb" | "pt" | "frames";

export interface DiagramBox {
  id: string;
  kind: DiagramBoxKind;
  label: string;
  subLabels: string[];
  x: number;
  y: number;
  width: number;
  height: number;
  // The color for this box's type or cache level. The actual border, fill,
  // and text colors are worked out later, so this just stores the data.
  hue: number;
  chroma: number;
  // How much lighter than this box's plain color to draw it, currently only
  // set for a Harvard level's instruction side. Left unset everywhere else.
  lightnessDelta?: number;
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
  hue: number;
  chroma: number;
  lightnessDelta?: number;
  clickable: boolean;
  targetTabId: string | null;
}

// A slot is either a single box, such as CPU or RAM, or two boxes stacked
// on top of each other, instruction on top and data on bottom.
type Slot = SlotBoxSpec[];

const BOX_WIDTH = 120;
const BOX_HEIGHT = 58;
const SLOT_GAP = 36;
const HARVARD_OFFSET = 42;
const TLB_WIDTH = 90;
const TLB_HEIGHT = 44;
const TLB_GAP = 24;
const TOP_PADDING = 10;
const BOTTOM_PADDING = 16;
const REG_WIDTH = 94;
const REG_HEIGHT = 28;
const REG_INSET_BOTTOM = 6;
const PAGE_WALK_GAP = 24;

// Under VM, RAM's box grows to fit its own header plus two nested boxes
// stacked below it, the page table and the frame pool.
const RAM_VM_WIDTH = 150;
const RAM_HEADER_HEIGHT = 46;
const RAM_SUB_GAP = 8;
const RAM_PT_SUB_HEIGHT = 30;
const RAM_FRAMES_SUB_HEIGHT = 46;
const RAM_VM_BOTTOM_PADDING = 6;
const RAM_SUB_INSET_X = 10;
const RAM_VM_HEIGHT = RAM_HEADER_HEIGHT + RAM_SUB_GAP + RAM_PT_SUB_HEIGHT + RAM_SUB_GAP + RAM_FRAMES_SUB_HEIGHT + RAM_VM_BOTTOM_PADDING;

// Draws a straight line when both ends already line up, otherwise bends
// the line through the midpoint so it only goes sideways or up and down.
function orthogonalPath(x1: number, y1: number, x2: number, y2: number): string {
  if (y1 === y2 || x1 === x2) return `M ${x1} ${y1} L ${x2} ${y2}`;
  const midX = (x1 + x2) / 2;
  return `M ${x1} ${y1} L ${midX} ${y1} L ${midX} ${y2} L ${x2} ${y2}`;
}

// Draws a path through a shared horizontal lane at laneY. Used for the
// line from the TLB to its page walk target, so it passes under the chain.
function laneRoutePath(x1: number, y1: number, laneY: number, x2: number, y2: number): string {
  return `M ${x1} ${y1} L ${x1} ${laneY} L ${x2} ${laneY} L ${x2} ${y2}`;
}

export function computeLayout(
  hierarchy: Hierarchy,
  ram: RamConfig,
  pageSize: number,
  disk: DiskConfig,
  tlb: TlbConfig,
  vmEnabled: boolean,
): DiagramLayout {
  // Only add the extra vertical space a split cache needs when one is
  // actually present, so a diagram without splits is not padded for nothing.
  const hasHarvard = hierarchy.some((level) => level.harvard);
  const centerYForHarvard = hasHarvard ? TOP_PADDING + HARVARD_OFFSET + BOX_HEIGHT / 2 : TOP_PADDING + BOX_HEIGHT / 2;
  // Under VM, RAM's own box is taller than the rest, so it needs its own
  // minimum center too, or its top edge could land above the top of the diagram.
  const centerYForRam = vmEnabled ? TOP_PADDING + RAM_VM_HEIGHT / 2 : 0;
  const CENTER_Y = Math.max(centerYForHarvard, centerYForRam);

  const slots: Slot[] = [];

  slots.push([
    { id: "cpu", kind: "cpu", label: "CPU", subLabels: [], hue: HUE_BY_KIND.cpu, chroma: TYPE_CHROMA, clickable: false, targetTabId: null },
  ]);

  hierarchy.forEach((level, index) => {
    const hue = cacheHue(index, hierarchy.length);
    if (level.harvard) {
      const iBytes = 2 ** level.i!.exp * level.i!.config.blockSize;
      const dBytes = 2 ** level.d!.exp * level.d!.config.blockSize;
      slots.push([
        {
          id: `${level.id}-i`,
          kind: "cache",
          label: chainLabel(hierarchy, index, "i"),
          subLabels: [`Size: ${byteSizeLabel(iBytes)}`, `Latency: ${cycleLabel(level.i!.config.hitTime)}`],
          hue,
          chroma: CACHE_CHROMA,
          lightnessDelta: HARVARD_INSTRUCTION_LIGHTNESS_DELTA,
          clickable: true,
          targetTabId: `${level.id}-i`,
        },
        {
          id: `${level.id}-d`,
          kind: "cache",
          label: chainLabel(hierarchy, index, "d"),
          subLabels: [`Size: ${byteSizeLabel(dBytes)}`, `Latency: ${cycleLabel(level.d!.config.hitTime)}`],
          hue,
          chroma: CACHE_CHROMA,
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
          subLabels: [`Size: ${byteSizeLabel(bytes)}`, `Latency: ${cycleLabel(level.unified!.config.hitTime)}`],
          hue,
          chroma: CACHE_CHROMA,
          clickable: true,
          targetTabId: level.id,
        },
      ]);
    }
  });

  // Under VM, the frame count moves into the frames box further down, so
  // RAM's own header shows just its latency, the same as when VM is off.
  const ramSubLabels = [`Latency: ${cycleLabel(ram.latency)}`];
  slots.push([
    { id: "ram", kind: "ram", label: "RAM", subLabels: ramSubLabels, hue: HUE_BY_KIND.ram, chroma: TYPE_CHROMA, clickable: true, targetTabId: "ram" },
  ]);

  if (vmEnabled) {
    slots.push([
      {
        id: "disk",
        kind: "disk",
        label: "DISK",
        subLabels: [`Latency: ${cycleLabel(disk.latency)}`],
        hue: HUE_BY_KIND.disk,
        chroma: TYPE_CHROMA,
        clickable: true,
        targetTabId: "disk",
      },
    ]);
  }

  // The first box, CPU, is never clickable, and neither is the last box,
  // except RAM and DISK, which always stay clickable.
  slots[0][0].clickable = false;
  slots[slots.length - 1].forEach((box) => {
    if (box.kind !== "ram" && box.kind !== "disk") box.clickable = false;
  });

  const boxes: DiagramBox[] = [];
  const slotX: number[] = [];
  const slotWidths: number[] = [];
  let cursorX = 2;
  slots.forEach((slot) => {
    slotX.push(cursorX);
    // Under VM, RAM's box is wider and taller than every other box, to
    // make room for its nested page table and frame boxes.
    const isVmRam = vmEnabled && slot.length === 1 && slot[0].kind === "ram";
    const boxWidth = isVmRam ? RAM_VM_WIDTH : BOX_WIDTH;
    const boxHeight = isVmRam ? RAM_VM_HEIGHT : BOX_HEIGHT;
    slotWidths.push(boxWidth);
    if (slot.length === 1) {
      boxes.push({ ...slot[0], x: cursorX, y: CENTER_Y - boxHeight / 2, width: boxWidth, height: boxHeight });
    } else {
      boxes.push({ ...slot[0], x: cursorX, y: CENTER_Y - HARVARD_OFFSET - BOX_HEIGHT / 2, width: BOX_WIDTH, height: BOX_HEIGHT });
      boxes.push({ ...slot[1], x: cursorX, y: CENTER_Y + HARVARD_OFFSET - BOX_HEIGHT / 2, width: BOX_WIDTH, height: BOX_HEIGHT });
    }
    cursorX += boxWidth + SLOT_GAP;
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
      // The number of TLB entries is always a power of two, just like cache
      // size, so this reuses the same K, M, G style formatting.
      subLabels: [`Entries: ${sizeLabel(Math.log2(tlb.entries))}`],
      x: tlbX,
      y: tlbY,
      width: TLB_WIDTH,
      height: TLB_HEIGHT,
      hue: HUE_BY_KIND.tlb,
      chroma: TYPE_CHROMA,
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
    const aRightX = slotX[i] + slotWidths[i];
    const bLeftX = slotX[i + 1];
    const topY = CENTER_Y - HARVARD_OFFSET;
    const bottomY = CENTER_Y + HARVARD_OFFSET;

    if (a.length === 1 && b.length === 1) {
      connectors.push({ id: `link-${i}`, path: orthogonalPath(aRightX, CENTER_Y, bLeftX, CENTER_Y) });
    } else if (a.length === 1 && b.length === 2) {
      connectors.push({ id: `link-${i}-top`, path: orthogonalPath(aRightX, CENTER_Y, bLeftX, topY) });
      connectors.push({ id: `link-${i}-bottom`, path: orthogonalPath(aRightX, CENTER_Y, bLeftX, bottomY) });
    } else if (a.length === 2 && b.length === 1) {
      connectors.push({ id: `link-${i}-top`, path: orthogonalPath(aRightX, topY, bLeftX, CENTER_Y) });
      connectors.push({ id: `link-${i}-bottom`, path: orthogonalPath(aRightX, bottomY, bLeftX, CENTER_Y) });
    } else {
      // Two split cache levels next to each other, so the instruction cache
      // connects to the next instruction cache and data to data, as two lines.
      connectors.push({ id: `link-${i}-top`, path: orthogonalPath(aRightX, topY, bLeftX, topY) });
      connectors.push({ id: `link-${i}-bottom`, path: orthogonalPath(aRightX, bottomY, bLeftX, bottomY) });
    }
  }

  const mainAxisBottomBase = hasHarvard ? CENTER_Y + HARVARD_OFFSET + BOX_HEIGHT / 2 : CENTER_Y + BOX_HEIGHT / 2;
  // RAM's box is taller than the rest under VM, so the bottom of the main
  // chain, and everything placed relative to it, has to account for that too.
  const mainAxisBottom = vmEnabled ? Math.max(mainAxisBottomBase, CENTER_Y + RAM_VM_HEIGHT / 2) : mainAxisBottomBase;
  const tlbBottom = tlbBox ? tlbBox.y + TLB_HEIGHT : 0;

  // Line from the TLB to its page walk target, only drawn when virtual
  // memory is on. If the target is a split cache level, it fans out to both boxes.
  let pageWalkLaneY = 0;
  if (vmEnabled && tlbBox) {
    const targetBoxes =
      tlb.pageWalkStart === "RAM"
        ? boxes.filter((b) => b.id === "ram")
        : boxes.filter((b) => b.id === tlb.pageWalkStart || b.id === `${tlb.pageWalkStart}-i` || b.id === `${tlb.pageWalkStart}-d`);

    if (targetBoxes.length > 0) {
      pageWalkLaneY = Math.max(mainAxisBottom, tlbBottom) + PAGE_WALK_GAP;
      const tlbCenterX = tlbBox.x + TLB_WIDTH / 2;
      const tlbBottomY = tlbBox.y + TLB_HEIGHT;
      targetBoxes.forEach((target) => {
        connectors.push({
          id: `pagewalk-${target.id}`,
          path: laneRoutePath(tlbCenterX, tlbBottomY, pageWalkLaneY, target.x + target.width / 2, target.y + target.height),
        });
      });
    }
  }

  // Once VM is on, RAM's box stops being clickable and instead shows two
  // nested boxes below its header, page table on top and frames below.
  const ramBoxForVm = boxes.find((b) => b.id === "ram") ?? null;
  if (vmEnabled && ramBoxForVm) {
    ramBoxForVm.clickable = false;
    const subWidth = ramBoxForVm.width - RAM_SUB_INSET_X * 2;
    const subX = ramBoxForVm.x + RAM_SUB_INSET_X;
    const ptY = ramBoxForVm.y + RAM_HEADER_HEIGHT + RAM_SUB_GAP;
    const framesY = ptY + RAM_PT_SUB_HEIGHT + RAM_SUB_GAP;
    boxes.push({
      id: "ram-pt",
      kind: "pt",
      label: "PT",
      // The page table has no real bounded size to report, so there is
      // nothing meaningful to put here.
      subLabels: [],
      x: subX,
      y: ptY,
      width: subWidth,
      height: RAM_PT_SUB_HEIGHT,
      hue: HUE_BY_KIND.pt,
      chroma: TYPE_CHROMA,
      clickable: true,
      targetTabId: "pageTable",
    });
    boxes.push({
      id: "ram-frames",
      kind: "frames",
      label: "Frames",
      // This is the frame pool's real byte capacity, frames times page size.
      // It does not include the page table, which has no physical size here.
      subLabels: [`Size: ${byteSizeLabel(ram.frames * pageSize)}`],
      x: subX,
      y: framesY,
      width: subWidth,
      height: RAM_FRAMES_SUB_HEIGHT,
      hue: HUE_BY_KIND.frames,
      chroma: TYPE_CHROMA,
      clickable: true,
      targetTabId: "frames",
    });
  }

  // REG sits inside the CPU box rather than being its own slot, so there is
  // no connecting line, just a smaller box added last so it draws on top.
  const cpuBox = boxes[0];
  boxes.push({
    id: "reg",
    kind: "reg",
    label: "REG",
    subLabels: [],
    x: cpuBox.x + (BOX_WIDTH - REG_WIDTH) / 2,
    y: cpuBox.y + BOX_HEIGHT - REG_HEIGHT - REG_INSET_BOTTOM,
    width: REG_WIDTH,
    height: REG_HEIGHT,
    hue: HUE_BY_KIND.reg,
    chroma: TYPE_CHROMA,
    clickable: true,
    targetTabId: null,
  });

  const width = cursorX - SLOT_GAP + 20;
  const height = Math.max(mainAxisBottom, tlbBottom, pageWalkLaneY) + BOTTOM_PADDING;

  return { width, height, boxes, connectors };
}
