import React from 'react';
import { logEvent } from '../utils/logger';
import { shouldAutoHideControls } from '../utils/menuHold';

/** How long the controls and the mouse pointer stay after the mouse stops moving while the video plays. */
export const CONTROLS_HIDE_DELAY_MS = 3000;

/**
 * Hides the controls, and with them the mouse pointer, after a few still seconds, but only while the
 * video plays and no menu is open: a paused video, or a menu the viewer is reading, keeps them in
 * view. Mouse movement (or `reveal`) shows them again; closing the menu starts the countdown anew.
 */
export function useControlsAutoHide(isPlaying: boolean, menuOpen: boolean, onHidden: () => void) {
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
    if (!shouldAutoHideControls(isPlaying, menuOpen)) return;
    timer.current = window.setTimeout(() => {
      shown.current = false;
      logEvent('debug', 'controls hidden after a few seconds without input');
      setVisible(false);
      hidden.current();
    }, CONTROLS_HIDE_DELAY_MS);
  }, [isPlaying, menuOpen]);

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
