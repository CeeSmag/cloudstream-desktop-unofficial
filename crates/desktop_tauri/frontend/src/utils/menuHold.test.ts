import { describe, expect, it } from 'vitest';
import { shouldAutoHideControls, videoClickAction } from './menuHold';

describe('menu hold', () => {
  it('hides the controls only while the video plays and no menu is open', () => {
    expect(shouldAutoHideControls(true, false)).toBe(true);
    expect(shouldAutoHideControls(true, true)).toBe(false);
    expect(shouldAutoHideControls(false, false)).toBe(false);
    expect(shouldAutoHideControls(false, true)).toBe(false);
  });

  it('closes an open menu on a click on the video without pausing it', () => {
    expect(videoClickAction(true)).toBe('close-menu');
    expect(videoClickAction(false)).toBe('toggle-play');
  });
});
