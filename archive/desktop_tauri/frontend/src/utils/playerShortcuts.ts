/** How far the left and right arrow keys jump, in milliseconds. */
export const KEY_SEEK_MS = 5000;
/** How much the up and down arrow keys change the volume, in percent. */
export const KEY_VOLUME_STEP = 5;

/** What a key press asks the player to do. */
export type Shortcut =
  | { type: 'seek'; deltaMs: number }
  | { type: 'volume'; delta: number }
  | { type: 'mute' }
  | { type: 'playpause' }
  | { type: 'fullscreen' };

/** The parts of a keyboard event that decide what it means. */
export interface KeyPress {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  /** The element that has the focus, if any. */
  target: { tagName?: string; type?: string; isContentEditable?: boolean } | null;
}

/** Input fields where typing letters and arrows must be left alone. */
const TYPING_TYPES = ['text', 'search', 'email', 'url', 'password', 'number', 'tel'];

function isTyping(target: KeyPress['target']): boolean {
  if (!target) return false;
  if (target.isContentEditable || target.tagName === 'TEXTAREA') return true;
  return target.tagName === 'INPUT' && TYPING_TYPES.includes(target.type ?? 'text');
}

/**
 * The player action for a key: Space pauses or plays, M mutes, F toggles fullscreen, the left and
 * right arrows go back and forward 5 seconds, the up and down arrows change the volume. Anything else, keys held with
 * Ctrl, Alt or Meta, and typing in a text field mean nothing here.
 */
export function shortcutFor(press: KeyPress): Shortcut | null {
  if (press.ctrlKey || press.altKey || press.metaKey || isTyping(press.target)) return null;
  switch (press.key) {
    case 'ArrowRight':
      return { type: 'seek', deltaMs: KEY_SEEK_MS };
    case 'ArrowLeft':
      return { type: 'seek', deltaMs: -KEY_SEEK_MS };
    case 'ArrowUp':
      return { type: 'volume', delta: KEY_VOLUME_STEP };
    case 'ArrowDown':
      return { type: 'volume', delta: -KEY_VOLUME_STEP };
    case ' ':
    case 'Spacebar':
      return { type: 'playpause' };
    case 'm':
    case 'M':
      return { type: 'mute' };
    case 'f':
    case 'F':
      return { type: 'fullscreen' };
    default:
      return null;
  }
}
