import { describe, expect, it } from 'vitest';
import { PlaybackStatus } from '../types';
import { INTENT_HOLD_MS, sameStatus, withPlayIntent } from './playIntent';

const status = (state: PlaybackStatus['state'], position_ms = 1000): PlaybackStatus => ({
  state,
  position_ms,
  duration_ms: 60_000,
  has_video: true,
  volume: 100,
  tracks: [],
  chapters: [],
  error: null,
});

describe('withPlayIntent', () => {
  it('shows the player state when the viewer has not just paused or played', () => {
    expect(withPlayIntent(status('Playing'), null, 5000).state).toBe('Playing');
  });

  it('a late answer that still says Playing does not undo a fresh pause', () => {
    const intent = { paused: true, at: 5000 };
    expect(withPlayIntent(status('Playing'), intent, 5001).state).toBe('Paused');
    expect(withPlayIntent(status('Playing'), intent, 5000 + INTENT_HOLD_MS - 1).state).toBe('Paused');
  });

  it('a late answer that still says Paused does not undo a fresh play', () => {
    expect(withPlayIntent(status('Paused'), { paused: false, at: 5000 }, 5200).state).toBe('Playing');
  });

  it('the player decides again once the hold is over', () => {
    const intent = { paused: true, at: 5000 };
    expect(withPlayIntent(status('Playing'), intent, 5000 + INTENT_HOLD_MS).state).toBe('Playing');
  });

  it('never hides loading, the end or a failure', () => {
    const intent = { paused: true, at: 5000 };
    for (const state of ['Loading', 'Ended', 'Failed', 'Idle'] as const) {
      expect(withPlayIntent(status(state), intent, 5001).state).toBe(state);
    }
  });

  it('keeps the position and returns the same object when nothing changes', () => {
    const playing = status('Playing', 42_000);
    const intent = { paused: true, at: 5000 };
    expect(withPlayIntent(playing, intent, 5001).position_ms).toBe(42_000);
    const paused = status('Paused');
    expect(withPlayIntent(paused, intent, 5001)).toBe(paused);
  });

  it('ignores an intent stamped in the future', () => {
    expect(withPlayIntent(status('Playing'), { paused: true, at: 9000 }, 5000).state).toBe('Playing');
  });
});

describe('sameStatus', () => {
  it('is true for equal statuses and false when anything differs', () => {
    expect(sameStatus(status('Paused'), status('Paused'))).toBe(true);
    expect(sameStatus(status('Paused'), status('Playing'))).toBe(false);
    expect(sameStatus(status('Paused', 1), status('Paused', 2))).toBe(false);
  });
});
