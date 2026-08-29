import type { DiskConfig, Hierarchy, RamConfig, TlbConfig } from "./model";
import { chainLabel, levelName, sizeLabel } from "./derive";

const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

// bytes is always an exact power of two here (entries and blockSize are both
// powers of two), so log2(bytes) is always a whole number.
export function byteSizeLabel(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const unitIndex = Math.min(Math.floor(Math.log2(bytes) / 10), SIZE_UNITS.length - 1);
  const value = bytes / 2 ** (unitIndex * 10);
  return `${value} ${SIZE_UNITS[unitIndex]}`;
}

const CYCLE_UNITS = ["", "K", "M", "B"];

// Cycle counts are plain decimal (base 1000), unlike byte counts — no
// power-of-two assumption here, so this rounds to one decimal place instead
// of relying on exact division.
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

// Cache levels get a hue gradient (warm at L1, cool at the deepest level)
// instead of a fixed hue, so an arbitrary number of levels stay visually
// distinct without hand-picking a color per level. Three anchors (warm ->
// mid -> cool) via two linear segments, per the design spec.
const CACHE_HUE_WARM = 45;
const CACHE_HUE_MID = 130;
const CACHE_HUE_COOL = 215;

export function cacheHue(levelIndex: number, totalLevels: number): number {
  if (totalLevels <= 1) return CACHE_HUE_WARM;
  const t = levelIndex / (totalLevels - 1);
  if (t < 0.5) return lerp(CACHE_HUE_WARM, CACHE_HUE_MID, t * 2);
  return lerp(CACHE_HUE_MID, CACHE_HUE_COOL, (t - 0.5) * 2);
}

// Fixed hues per box kind (OKLCH), from the design spec. Cache is excluded
// here — its hue is computed per level by cacheHue instead of being fixed.
const TYPE_CHROMA = 0.07;
const CACHE_CHROMA = 0.09;
const HUE_BY_KIND: Record<Exclude<DiagramBoxKind, "cache">, number> = {
  cpu: 15,
  reg: 95,
  tlb: 330,
  ram: 255,
  disk: 290,
};

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
  // OKLCH hue (0-360) and chroma for this box's type/cache-level color.
  // Resolved to actual border/fill/text colors in CSS (theme-aware, via
  // shared lightness tokens), not here — this stays a pure data function.
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

// One slot is a single box (CPU/REG/unified cache/RAM/DISK) or two stacked
// boxes (a Harvard-split cache level: i on top, d on bottom).
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

// Right-angle routing: a straight line when both ends already share an axis,
// otherwise a horizontal-vertical-horizontal dogleg through the midpoint —
// no diagonals, per the design spec.
function orthogonalPath(x1: number, y1: number, x2: number, y2: number): string {
  if (y1 === y2 || x1 === x2) return `M ${x1} ${y1} L ${x2} ${y2}`;
  const midX = (x1 + x2) / 2;
  return `M ${x1} ${y1} L ${midX} ${y1} L ${midX} ${y2} L ${x2} ${y2}`;
}

// Routes through a shared horizontal lane at laneY — used for the TLB ->
// page-walk-target connector, which must cross underneath the whole main
// chain rather than cut across it.
function laneRoutePath(x1: number, y1: number, laneY: number, x2: number, y2: number): string {
  return `M ${x1} ${y1} L ${x1} ${laneY} L ${x2} ${laneY} L ${x2} ${y2}`;
}

export function computeLayout(
  hierarchy: Hierarchy,
  ram: RamConfig,
  disk: DiskConfig,
  tlb: TlbConfig,
  vmEnabled: boolean,
): DiagramLayout {
  // Only reserve the extra vertical room a Harvard split needs when one is
  // actually present, so the common (no-split) diagram isn't padded for a
  // shape it doesn't have.
  const hasHarvard = hierarchy.some((level) => level.harvard);
  const CENTER_Y = hasHarvard ? TOP_PADDING + HARVARD_OFFSET + BOX_HEIGHT / 2 : TOP_PADDING + BOX_HEIGHT / 2;

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

  const ramSubLabels = vmEnabled
    ? [`Size: ${byteSizeLabel(ram.size)}`, `Latency: ${cycleLabel(ram.latency)}`]
    : [`Latency: ${cycleLabel(ram.latency)}`];
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

  // First (CPU) slot in the main chain is never clickable. The last (terminal)
  // slot is also never clickable, except RAM, which must stay clickable
  // regardless of whether it ends the chain (i.e. VM disabled) or not.
  slots[0][0].clickable = false;
  slots[slots.length - 1].forEach((box) => {
    if (box.kind !== "ram") box.clickable = false;
  });

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
      // TLB entries is always a power of two, same convention as cache
      // "size" — reuse sizeLabel (K/M/G/... entry-count formatting) rather
      // than the raw number, which can otherwise run into the thousands.
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
    const aRightX = slotX[i] + BOX_WIDTH;
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
      // Two adjacent Harvard-split levels: instruction cache connects
      // straight across to instruction cache, data to data — two parallel
      // lines, not a pinch-merge through the center.
      connectors.push({ id: `link-${i}-top`, path: orthogonalPath(aRightX, topY, bLeftX, topY) });
      connectors.push({ id: `link-${i}-bottom`, path: orthogonalPath(aRightX, bottomY, bLeftX, bottomY) });
    }
  }

  const mainAxisBottom = hasHarvard ? CENTER_Y + HARVARD_OFFSET + BOX_HEIGHT / 2 : CENTER_Y + BOX_HEIGHT / 2;
  const tlbBottom = tlbBox ? tlbBox.y + TLB_HEIGHT : 0;

  // TLB -> page-walk-target: only meaningful with VM on, and only once the
  // TLB box exists. Routes underneath the whole main chain through a shared
  // lane rather than cutting diagonally across other boxes. A Harvard-split
  // target fans out to both its i and d boxes.
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

  // REG nests inside the CPU box (not its own slot) — no separate connector,
  // just a smaller box inset near the bottom of CPU's rect. Pushed last so
  // it draws on top of the CPU rect.
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
