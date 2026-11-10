import React from 'react';
import { KeyPress, shortcutFor } from '../utils/playerShortcuts';

/** What the player does for each shortcut. */
export interface ShortcutActions {
  togglePlay: () => void;
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
    const read = (event: KeyboardEvent) =>
      shortcutFor({
        key: event.key,
        ctrlKey: event.ctrlKey,
        altKey: event.altKey,
        metaKey: event.metaKey,
        target: event.target as KeyPress['target'],
      });
    const onKey = (event: KeyboardEvent) => {
      const shortcut = read(event);
      if (!shortcut) return;
      // Also keeps Space from scrolling the page.
      event.preventDefault();
      // A held Space pauses once; it does not flip between paused and playing.
      if (shortcut.type === 'playpause' && event.repeat) return;
      const current = latest.current;
      current.onActivity();
      if (shortcut.type === 'seek') current.seekBy(shortcut.deltaMs);
      else if (shortcut.type === 'volume') current.volumeBy(shortcut.delta);
      else if (shortcut.type === 'mute') current.toggleMute();
      else if (shortcut.type === 'playpause') current.togglePlay();
      else current.toggleFullscreen();
    };
    // Letting go of Space "clicks" a button that has the focus (the last one clicked); the key
    // already paused or played, so that click is stopped.
    const onKeyUp = (event: KeyboardEvent) => {
      if (read(event)?.type === 'playpause') event.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('keyup', onKeyUp);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('keyup', onKeyUp);
    };
  }, []);
}
