/** How far the position must move before mpv is known to be painting. */
export const PAINT_PROOF_MS = 150;
/** For a video that starts paused (the position never moves): the longest wait before showing it. */
export const PAINT_TIMEOUT_MS = 1500;

/**
 * True once the page may turn see-through: mpv's own window is under it, but only after mpv has
 * painted its first picture. Before that the desktop would show through the transparent window.
 * `startMs` is the position when playback became active, `nowMs` the position now.
 */
export function videoHasPainted(startMs: number, nowMs: number): boolean {
  return nowMs - startMs >= PAINT_PROOF_MS;
}
