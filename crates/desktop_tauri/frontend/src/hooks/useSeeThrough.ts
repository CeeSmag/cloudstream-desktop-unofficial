import { useEffect, useRef, useState } from 'react';
import type { PlaybackStatus } from '../types';
import { PAINT_TIMEOUT_MS, videoHasPainted } from '../utils/seeThrough';

/**
 * Whether the player page should be see-through so mpv's window under it shows. It stays opaque
 * while loading and until the first picture is painted (the position moves, or a timeout for a
 * video that starts paused); otherwise the transparent window would show the desktop.
 */
export function useSeeThrough(native: boolean, status: PlaybackStatus): boolean {
  const active =
    native && status.has_video && (status.state === 'Playing' || status.state === 'Paused');
  const [painted, setPainted] = useState(false);
  const startMs = useRef(0);

  useEffect(() => {
    if (!active) {
      setPainted(false);
      return undefined;
    }
    startMs.current = status.position_ms;
    const timer = window.setTimeout(() => setPainted(true), PAINT_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
    // The start position is read once, when playback becomes active.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useEffect(() => {
    if (active && videoHasPainted(startMs.current, status.position_ms)) setPainted(true);
  }, [active, status.position_ms]);

  return active && painted;
}
