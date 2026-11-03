import { describe, expect, it } from 'vitest';
import { KeyPress, shortcutFor } from './playerShortcuts';

const press = (key: string, over: Partial<KeyPress> = {}): KeyPress => ({
  key,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
  target: null,
  ...over,
});

describe('shortcutFor', () => {
  it('maps the arrows to 5 second jumps and volume steps', () => {
    expect(shortcutFor(press('ArrowRight'))).toEqual({ type: 'seek', deltaMs: 5000 });
    expect(shortcutFor(press('ArrowLeft'))).toEqual({ type: 'seek', deltaMs: -5000 });
    expect(shortcutFor(press('ArrowUp'))).toEqual({ type: 'volume', delta: 5 });
    expect(shortcutFor(press('ArrowDown'))).toEqual({ type: 'volume', delta: -5 });
  });

  it('maps M to mute and F to fullscreen, in either case', () => {
    expect(shortcutFor(press('m'))).toEqual({ type: 'mute' });
    expect(shortcutFor(press('M'))).toEqual({ type: 'mute' });
    expect(shortcutFor(press('f'))).toEqual({ type: 'fullscreen' });
    expect(shortcutFor(press('F'))).toEqual({ type: 'fullscreen' });
  });

  it('ignores other keys and keys held with Ctrl, Alt or Meta', () => {
    expect(shortcutFor(press('x'))).toBeNull();
    expect(shortcutFor(press('f', { ctrlKey: true }))).toBeNull();
    expect(shortcutFor(press('ArrowRight', { altKey: true }))).toBeNull();
    expect(shortcutFor(press('m', { metaKey: true }))).toBeNull();
  });

  it('leaves typing in a text field alone but works on the volume slider', () => {
    expect(shortcutFor(press('f', { target: { tagName: 'INPUT', type: 'text' } }))).toBeNull();
    expect(shortcutFor(press('m', { target: { tagName: 'TEXTAREA' } }))).toBeNull();
    expect(shortcutFor(press('ArrowUp', { target: { tagName: 'INPUT', type: 'range' } }))).toEqual({
      type: 'volume',
      delta: 5,
    });
  });
});
