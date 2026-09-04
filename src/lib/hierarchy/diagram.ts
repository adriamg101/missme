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
// deepest level, instead of one fixed color, so any number of levels stay
// visually distinct without picking a color for each one by hand. It blends
// through three color anchors (warm, mid, cool), per the design spec.
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
const TYPE_CHROMA = 0.07;
const CACHE_CHROMA = 0.09;
const HUE_BY_KIND: Record<Exclude<DiagramBoxKind, "cache">, number> = {
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
  // The color (hue and chroma) for this box's type or cache level. The
  // actual border, fill, and text colors are worked out later in CSS, so
  // this just stores the data, not the final look.
  hue: number;
  chroma: number;
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
  clickable: boolean;
  targetTabId: string | null;
}

// A slot is either a single box (CPU, REG, a shared cache, RAM, DISK) or
// two boxes stacked on top of each other (a split instruction/data cache
// level, instruction on top, data on bottom).
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

// Under VM, RAM's box grows to fit its own header (label + latency) plus
// two nested boxes stacked below it: PT (just a label, since the page
// table has no real bounded size to report) and Frames (label plus its
// real byte capacity). It ends up taller and a bit wider than every other
// box in the diagram, which is fine, nothing else needs to match it.
const RAM_VM_WIDTH = 150;
const RAM_HEADER_HEIGHT = 46;
const RAM_SUB_GAP = 8;
const RAM_PT_SUB_HEIGHT = 30;
const RAM_FRAMES_SUB_HEIGHT = 46;
const RAM_VM_BOTTOM_PADDING = 6;
const RAM_SUB_INSET_X = 10;
const RAM_VM_HEIGHT = RAM_HEADER_HEIGHT + RAM_SUB_GAP + RAM_PT_SUB_HEIGHT + RAM_SUB_GAP + RAM_FRAMES_SUB_HEIGHT + RAM_VM_BOTTOM_PADDING;

// Right-angle routing: draw a straight line when both ends already line up,
// otherwise bend the line through the midpoint so it only goes sideways and
// up or down, never diagonally, per the design spec.
function orthogonalPath(x1: number, y1: number, x2: number, y2: number): string {
  if (y1 === y2 || x1 === x2) return `M ${x1} ${y1} L ${x2} ${y2}`;
  const midX = (x1 + x2) / 2;
  return `M ${x1} ${y1} L ${midX} ${y1} L ${midX} ${y2} L ${x2} ${y2}`;
}

// Draws a path that travels through a shared horizontal lane at laneY.
// Used for the line from the TLB to its page-walk target, since that line
// needs to pass underneath the whole main chain instead of cutting across it.
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
  // actually present, so a diagram without any splits isn't padded for a
  // shape it doesn't have.
  const hasHarvard = hierarchy.some((level) => level.harvard);
  const centerYForHarvard = hasHarvard ? TOP_PADDING + HARVARD_OFFSET + BOX_HEIGHT / 2 : TOP_PADDING + BOX_HEIGHT / 2;
  // Under VM, RAM's own box is taller than the rest (see RAM_VM_HEIGHT
  // above), so it needs its own minimum center too - otherwise, with no
  // split cache pushing the center down, RAM's top edge would land above
  // y=0 and get clipped by the diagram's own top edge.
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

  // Under VM, RAM's own frame count moves into the Frames sub-box's own
  // "Size" line below (see the ram-frames box further down), so RAM's own
  // header shows just its latency, the same as when VM is off.
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

  // The first box (CPU) in the main chain is never clickable, and neither
  // is the last box, except RAM and DISK, which always stay clickable
  // whether or not they happen to be the last box in the chain.
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
    // Under VM, RAM's box is wider and taller than every other box (see
    // RAM_VM_WIDTH/RAM_VM_HEIGHT above), to make room for its nested PT and
    // Frames sub-boxes.
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
      // size, so this reuses the same K/M/G-style formatting instead of the
      // raw number, which can otherwise run into the thousands.
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
      // Two split cache levels next to each other: the instruction cache
      // connects straight to the next instruction cache, and the data cache
      // to the next data cache, as two separate parallel lines instead of
      // merging in the middle.
      connectors.push({ id: `link-${i}-top`, path: orthogonalPath(aRightX, topY, bLeftX, topY) });
      connectors.push({ id: `link-${i}-bottom`, path: orthogonalPath(aRightX, bottomY, bLeftX, bottomY) });
    }
  }

  const mainAxisBottomBase = hasHarvard ? CENTER_Y + HARVARD_OFFSET + BOX_HEIGHT / 2 : CENTER_Y + BOX_HEIGHT / 2;
  // RAM's box is taller than the rest under VM, so the bottom of the main
  // chain (and everything positioned relative to it, like the page-walk
  // lane below) has to account for that too, not just the shared BOX_HEIGHT.
  const mainAxisBottom = vmEnabled ? Math.max(mainAxisBottomBase, CENTER_Y + RAM_VM_HEIGHT / 2) : mainAxisBottomBase;
  const tlbBottom = tlbBox ? tlbBox.y + TLB_HEIGHT : 0;

  // Line from the TLB to its page-walk target: only drawn when virtual
  // memory is on and the TLB box exists. It runs underneath the whole main
  // chain through a shared lane instead of cutting diagonally across other
  // boxes. If the target is a split cache level, the line fans out to both
  // its instruction and data boxes.
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

  // Once VM is on, RAM's own box stops being directly clickable and instead
  // shows two nested boxes stacked below its own header (page table on top,
  // frames below, since the page table is what's consulted first), each
  // routing to its own view. RAM's box was already sized to fit them (see
  // RAM_VM_HEIGHT above), so this only has to place them, not resize RAM.
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
      // The page table has no real bounded size to report (see the PT
      // view's own design), so there's nothing meaningful to put here.
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
      // This is the frame pool's real byte capacity (frames * page size);
      // it deliberately doesn't include the page table, which has no
      // physical size of its own in this model.
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

  // REG sits inside the CPU box rather than being its own slot, so there's
  // no separate connecting line, just a smaller box placed near the bottom
  // of the CPU box. It's added last so it's drawn on top of the CPU box.
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
