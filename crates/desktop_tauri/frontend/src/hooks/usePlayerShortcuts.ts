import React from 'react';
import { KeyPress, shortcutFor } from '../utils/playerShortcuts';

/** What the player does for each shortcut. */
export interface ShortcutActions {
  seekBy: (deltaMs: number) => void;
  volumeBy: (delta: number) => void;
  toggleMute: () => void;
  toggleFullscreen: () => void;
  /** Called for every shortcut, so the controls show what changed. */
  onActivity: () => void;
}

/** Keyboard shortcuts of the player, active while it is on screen. */
export function usePlayerShortcuts(actions: ShortcutActions): void {
  const latest = React.useRef(actions);
  latest.current = actions;

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const shortcut = shortcutFor({
        key: event.key,
        ctrlKey: event.ctrlKey,
        altKey: event.altKey,
        metaKey: event.metaKey,
        target: event.target as KeyPress['target'],
      });
      if (!shortcut) return;
      event.preventDefault();
      const current = latest.current;
      current.onActivity();
      if (shortcut.type === 'seek') current.seekBy(shortcut.deltaMs);
      else if (shortcut.type === 'volume') current.volumeBy(shortcut.delta);
      else if (shortcut.type === 'mute') current.toggleMute();
      else current.toggleFullscreen();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
