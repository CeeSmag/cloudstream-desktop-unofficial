import React from 'react';
import { logEvent } from '../utils/logger';

/** How long the controls and the mouse pointer stay after the mouse stops moving while the video plays. */
export const CONTROLS_HIDE_DELAY_MS = 3000;

/**
 * Hides the controls, and with them the mouse pointer, after a few still seconds, but only while the
 * video plays. A paused video keeps them in view. Mouse movement (or `reveal`) shows them again.
 */
export function useControlsAutoHide(isPlaying: boolean, onHidden: () => void) {
  const [visible, setVisible] = React.useState(true);
  const shown = React.useRef(true);
  const timer = React.useRef<number | null>(null);
  const hidden = React.useRef(onHidden);
  hidden.current = onHidden;

  const reveal = React.useCallback(() => {
    if (!shown.current) logEvent('debug', 'controls shown again: the page received input');
    shown.current = true;
    setVisible(true);
    if (timer.current !== null) window.clearTimeout(timer.current);
    if (!isPlaying) return;
    timer.current = window.setTimeout(() => {
      shown.current = false;
      logEvent('debug', 'controls hidden after a few seconds without input');
      setVisible(false);
      hidden.current();
    }, CONTROLS_HIDE_DELAY_MS);
  }, [isPlaying]);

  React.useEffect(() => {
    reveal();
    window.addEventListener('mousemove', reveal);
    return () => {
      window.removeEventListener('mousemove', reveal);
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [reveal]);

  return { visible, reveal };
}
