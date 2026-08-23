const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

// bytes is always an exact power of two here (entries and blockSize are both
// powers of two), so log2(bytes) is always a whole number.
export function byteSizeLabel(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const unitIndex = Math.min(Math.floor(Math.log2(bytes) / 10), SIZE_UNITS.length - 1);
  const value = bytes / 2 ** (unitIndex * 10);
  return `${value} ${SIZE_UNITS[unitIndex]}`;
}
