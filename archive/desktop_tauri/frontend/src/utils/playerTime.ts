/** Formats a position in milliseconds as `m:ss`, or `h:mm:ss` from one hour up. */
export function formatTime(milliseconds: number): string {
  const totalSeconds = Math.floor(Math.max(0, milliseconds) / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  const hours = Math.floor(minutes / 60);
  const paddedSeconds = String(seconds).padStart(2, '0');
  if (hours > 0) {
    return `${hours}:${String(minutes % 60).padStart(2, '0')}:${paddedSeconds}`;
  }
  return `${minutes}:${paddedSeconds}`;
}

/** How far along playback is, clamped to 0–100; 0 while the duration is unknown. */
export function progressPercent(positionMs: number, durationMs: number): number {
  if (durationMs <= 0) return 0;
  return Math.min(100, Math.max(0, (positionMs / durationMs) * 100));
}

/** Turns a click position on the seek bar into a playback position in milliseconds. */
export function seekPositionFromClick(
  clickX: number,
  barLeft: number,
  barWidth: number,
  durationMs: number,
): number {
  if (barWidth <= 0) return 0;
  const ratio = Math.max(0, Math.min(1, (clickX - barLeft) / barWidth));
  return Math.floor(ratio * durationMs);
}
