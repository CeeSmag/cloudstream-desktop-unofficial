import React from 'react';
import { api } from '../services/api';
import { describeValue, logEvent } from '../utils/logger';

/**
 * Fullscreen for the player, done on the native window. The browser's own fullscreen is not used:
 * inside the webview it can leave the mouse stuck while a video plays. Escape leaves fullscreen,
 * and so does leaving the player.
 */
export function useWindowFullscreen() {
  const [fullscreen, setFullscreen] = React.useState(false);
  const current = React.useRef(false);

  const change = React.useCallback(async (next: boolean) => {
    try {
      await api.setWindowFullscreen(next);
      current.current = next;
      setFullscreen(next);
      logEvent('info', `window fullscreen ${next ? 'on' : 'off'}`);
    } catch (error) {
      logEvent('warn', `could not change fullscreen to ${next}: ${describeValue(error)}`);
    }
  }, []);

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && current.current) void change(false);
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      if (current.current) void api.setWindowFullscreen(false).catch(() => undefined);
    };
  }, [change]);

  const toggle = React.useCallback(() => change(!current.current), [change]);
  return { fullscreen, toggle };
}
