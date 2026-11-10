import { describe, expect, it } from 'vitest';
import { ARRIVED_MS, seekTarget, SETTLE_MS, shownPosition } from './seekTarget';

const base = { positionMs: 100_000, durationMs: 1_000_000, deltaMs: 5000, pending: null, now: 10_000 };

describe('seekTarget', () => {
  it('starts from the reported position when no jump is on its way', () => {
    expect(seekTarget(base)).toBe(105_000);
  });

  it('two quick presses add up even though the reported position has not moved yet', () => {
    const first = seekTarget(base);
    const second = seekTarget({ ...base, pending: { targetMs: first, at: 10_000 }, now: 10_300 });
    expect(second).toBe(110_000);
    const third = seekTarget({ ...base, pending: { targetMs: second, at: 10_300 }, now: 10_600 });
    expect(third).toBe(115_000);
  });

  it('goes back from the pending target when the keys are mixed', () => {
    const pending = { targetMs: 110_000, at: 10_000 };
    expect(seekTarget({ ...base, deltaMs: -5000, pending, now: 10_200 })).toBe(105_000);
  });

  it('trusts the reported position again once the jump has settled', () => {
    const pending = { targetMs: 110_000, at: 10_000 };
    expect(seekTarget({ ...base, positionMs: 111_000, pending, now: 10_000 + SETTLE_MS - 1 })).toBe(115_000);
    expect(seekTarget({ ...base, positionMs: 111_000, pending, now: 10_000 + SETTLE_MS })).toBe(116_000);
  });

  it('ignores a pending jump stamped in the future', () => {
    expect(seekTarget({ ...base, pending: { targetMs: 900_000, at: 20_000 } })).toBe(105_000);
  });

  it('stays between the start and the end of the video', () => {
    expect(seekTarget({ ...base, positionMs: 2000, deltaMs: -5000 })).toBe(0);
    expect(seekTarget({ ...base, positionMs: 998_000, deltaMs: 5000 })).toBe(1_000_000);
    expect(seekTarget({ ...base, durationMs: 0 })).toBe(105_000);
  });
});

describe('shownPosition', () => {
  const pending = { targetMs: 110_000, at: 10_000 };

  it('shows the reported position when no jump is on its way', () => {
    expect(shownPosition({ positionMs: 100_000, pending: null, now: 10_000 })).toBe(100_000);
  });

  it('shows the target of a jump at once, while the player still reports the old place', () => {
    expect(shownPosition({ positionMs: 100_000, pending, now: 10_000 })).toBe(110_000);
    expect(shownPosition({ positionMs: 100_000, pending, now: 10_000 + SETTLE_MS - 1 })).toBe(110_000);
  });

  it('follows the player again as soon as it has arrived near the target', () => {
    expect(shownPosition({ positionMs: 110_400, pending, now: 10_300 })).toBe(110_400);
    expect(shownPosition({ positionMs: 110_000 - ARRIVED_MS, pending, now: 10_300 })).toBe(110_000 - ARRIVED_MS);
    expect(shownPosition({ positionMs: 110_000 - ARRIVED_MS - 1, pending, now: 10_300 })).toBe(110_000);
  });

  it('trusts the player once the jump has had its time, wherever it is', () => {
    expect(shownPosition({ positionMs: 100_000, pending, now: 10_000 + SETTLE_MS })).toBe(100_000);
  });
});
