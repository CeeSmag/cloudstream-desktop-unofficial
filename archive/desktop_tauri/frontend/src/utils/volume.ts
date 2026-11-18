export const DEFAULT_VOLUME = 100;
const STORAGE_KEY = 'cloudstream.volume';

/** Whole percent between 0 and 100; anything that is not a number falls back to the default. */
export function clampVolume(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_VOLUME;
  return Math.round(Math.min(100, Math.max(0, value)));
}

/** Reads a stored volume; missing or damaged values give the default. */
export function parseStoredVolume(raw: string | null): number {
  if (raw === null || raw.trim() === '') return DEFAULT_VOLUME;
  return clampVolume(Number(raw));
}

/** The volume remembered from the last session (storage may be unavailable). */
export function loadVolume(): number {
  try {
    return parseStoredVolume(localStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_VOLUME;
  }
}

export function saveVolume(volume: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(clampVolume(volume)));
  } catch {
    // Remembering the volume is a convenience; playback does not depend on it.
  }
}
