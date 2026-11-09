/** The sizes (in GB) the data folder limit can be set to, smallest first. */
export const STORAGE_CAP_OPTIONS: number[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/** What the limit is until the viewer chooses. */
export const DEFAULT_STORAGE_CAP_GB = 2;

const BYTES_PER_GB = 1024 * 1024 * 1024;

/** The limit in bytes. */
export function capBytes(gb: number): number {
  return gb * BYTES_PER_GB;
}

/** The share of the limit that is used, 0 to 100 (more than the limit shows as 100). */
export function usedPercent(usedBytes: number, gb: number): number {
  if (gb <= 0) return 100;
  return Math.min(100, Math.round((usedBytes / capBytes(gb)) * 100));
}

/** "2 GB" for a drop-down entry. */
export function capLabel(gb: number): string {
  return `${gb} GB`;
}
