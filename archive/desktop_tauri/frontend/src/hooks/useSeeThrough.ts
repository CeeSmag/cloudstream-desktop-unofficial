import { useEffect, useReducer } from 'react';
import type { PlaybackStatus } from '../types';
import { IDLE_PAINT, isSeeThrough, PAINT_TIMEOUT_MS, paintStep } from '../utils/seeThrough';

/**
 * Whether the player page should be see-through so mpv's window under it shows. It stays opaque
 * while loading and until the first picture is painted (the position moves, or a timeout for a
 * video that starts paused); otherwise the transparent window would show the desktop. The rules
 * are in `paintStep`, which is tested; this hook only feeds it.
 */
export function useSeeThrough(native: boolean, status: PlaybackStatus): boolean {
  const active =
    native && status.has_video && (status.state === 'Playing' || status.state === 'Paused');
  const positionMs = status.position_ms;
  const [state, dispatch] = useReducer(paintStep, IDLE_PAINT);

  useEffect(() => {
    dispatch({ type: 'activity', active, positionMs });
    // The start position is read once, when playback becomes active or stops.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useEffect(() => {
    dispatch({ type: 'position', positionMs });
  }, [positionMs]);

  useEffect(() => {
    if (!state.active) return undefined;
    const { generation } = state;
    const timer = window.setTimeout(() => dispatch({ type: 'timeout', generation }), PAINT_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [state.active, state.generation]);

  // `active` is checked here too: the state catches up one render after playback stops.
  return active && isSeeThrough(state);
}
