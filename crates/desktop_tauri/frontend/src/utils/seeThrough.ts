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

/** Where the wait for the first picture stands. */
export interface PaintState {
  /** A video that can show through is playing or paused. */
  active: boolean;
  /** The position when this playback became active. */
  startMs: number;
  /** The first picture is known to be painted. */
  painted: boolean;
  /** Counts the playbacks, so a timeout of an earlier one is recognised and ignored. */
  generation: number;
}

export const IDLE_PAINT: PaintState = { active: false, startMs: 0, painted: false, generation: 0 };

export type PaintEvent =
  | { type: 'activity'; active: boolean; positionMs: number }
  | { type: 'position'; positionMs: number }
  | { type: 'timeout'; generation: number };

/** One step of the wait: pure, so every order of events can be tested without a window. */
export function paintStep(state: PaintState, event: PaintEvent): PaintState {
  switch (event.type) {
    case 'activity':
      if (!event.active) return state.active ? { ...IDLE_PAINT, generation: state.generation } : state;
      if (state.active) return state;
      return { active: true, startMs: event.positionMs, painted: false, generation: state.generation + 1 };
    case 'position':
      if (!state.active || state.painted || !videoHasPainted(state.startMs, event.positionMs)) return state;
      return { ...state, painted: true };
    case 'timeout':
      if (!state.active || event.generation !== state.generation) return state;
      return { ...state, painted: true };
  }
}

/** Whether the page should be see-through now. */
export function isSeeThrough(state: PaintState): boolean {
  return state.active && state.painted;
}
