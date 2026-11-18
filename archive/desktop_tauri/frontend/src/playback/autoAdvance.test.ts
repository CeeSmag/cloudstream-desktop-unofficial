import { describe, expect, it } from 'vitest';
import { shouldAutoAdvance } from './autoAdvance';

const ended = { state: 'Ended' as const, position_ms: 1000, duration_ms: 1000 };
const run = (overrides: Partial<Parameters<typeof shouldAutoAdvance>[0]> = {}) =>
  shouldAutoAdvance({ status: ended, hasNext: true, enabled: true, ...overrides });

describe('shouldAutoAdvance', () => {
  it('advances when the stream ended at its end and there is a next episode', () => {
    expect(run()).toBe(true);
  });

  it('does nothing while the video is still playing, paused or loading', () => {
    for (const state of ['Playing', 'Paused', 'Loading', 'Idle', 'Failed'] as const) {
      expect(run({ status: { ...ended, state } })).toBe(false);
    }
  });

  it('does nothing after the last episode or when the viewer switched it off', () => {
    expect(run({ hasNext: false })).toBe(false);
    expect(run({ enabled: false })).toBe(false);
  });

  it('counts exactly 90 percent as the end and 89.9 percent as a stream cut short', () => {
    expect(run({ status: { ...ended, position_ms: 900 } })).toBe(true);
    expect(run({ status: { ...ended, position_ms: 899 } })).toBe(false);
  });

  it('does nothing when the length is unknown', () => {
    expect(run({ status: { ...ended, duration_ms: 0, position_ms: 0 } })).toBe(false);
  });
});
