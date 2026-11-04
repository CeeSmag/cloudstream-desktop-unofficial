import { describe, expect, it } from 'vitest';
import { createSeekScheduler, MAX_WAIT_MS, QUIET_MS, Timers } from './seekScheduler';

/** A clock and timers that only move when the test says so. */
function fakeTimers() {
  let time = 0;
  let nextId = 1;
  const pending = new Map<number, { at: number; action: () => void }>();
  const timers: Timers = {
    now: () => time,
    setTimer: (action, ms) => {
      const id = nextId++;
      pending.set(id, { at: time + ms, action });
      return id;
    },
    clearTimer: (handle) => {
      pending.delete(handle as number);
    },
  };
  const advance = (ms: number) => {
    const end = time + ms;
    for (;;) {
      const due = [...pending.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      time = due[1].at;
      pending.delete(due[0]);
      due[1].action();
    }
    time = end;
  };
  return { timers, advance, waiting: () => pending.size };
}

function rig() {
  const sent: number[] = [];
  const clock = fakeTimers();
  const scheduler = createSeekScheduler((ms) => sent.push(ms), clock.timers);
  return { sent, scheduler, ...clock };
}

describe('createSeekScheduler', () => {
  it('sends nothing until the keys have been quiet, then sends the last target once', () => {
    const { sent, scheduler, advance } = rig();
    scheduler.press(5000);
    advance(100);
    scheduler.press(10_000);
    advance(100);
    scheduler.press(15_000);
    advance(QUIET_MS - 1);
    expect(sent).toEqual([]);
    advance(1);
    expect(sent).toEqual([15_000]);
  });

  it('sends exactly at the quiet time after a single press', () => {
    const { sent, scheduler, advance } = rig();
    scheduler.press(7000);
    advance(QUIET_MS);
    expect(sent).toEqual([7000]);
  });

  it('a held key (a press every 200 ms) still sends about once a second, never a flood', () => {
    const { sent, scheduler, advance } = rig();
    for (let target = 5000, n = 0; n < 25; n++, target += 5000) {
      scheduler.press(target);
      advance(200);
    }
    advance(QUIET_MS);
    expect(sent.length).toBeLessThanOrEqual(6);
    expect(sent.length).toBeGreaterThanOrEqual(4);
    expect(sent[sent.length - 1]).toBe(125_000);
    expect([...sent].sort((a, b) => a - b)).toEqual(sent);
  });

  it('sends at the longest wait even while the keys never go quiet', () => {
    const { sent, scheduler, advance } = rig();
    // A press every 200 ms is always inside the quiet time (250 ms), so only the longest wait sends.
    for (let n = 1; n <= 5; n++) {
      scheduler.press(n);
      advance(200);
    }
    expect(sent).toEqual([]);
    expect(5 * 200).toBe(MAX_WAIT_MS); // the sixth press comes exactly at the longest wait
    scheduler.press(6);
    expect(sent).toEqual([6]);
  });

  it('sendNow sends at once and drops a jump that was waiting', () => {
    const { sent, scheduler, advance, waiting } = rig();
    scheduler.press(5000);
    scheduler.sendNow(90_000);
    expect(sent).toEqual([90_000]);
    expect(waiting()).toBe(0);
    advance(5000);
    expect(sent).toEqual([90_000]);
  });

  it('cancel forgets a waiting jump without sending it', () => {
    const { sent, scheduler, advance, waiting } = rig();
    scheduler.press(5000);
    scheduler.cancel();
    advance(5000);
    expect(sent).toEqual([]);
    expect(waiting()).toBe(0);
  });

  it('starts a fresh wait for the next burst after a send', () => {
    const { sent, scheduler, advance } = rig();
    scheduler.press(5000);
    advance(QUIET_MS);
    advance(5000);
    scheduler.press(10_000);
    advance(QUIET_MS);
    expect(sent).toEqual([5000, 10_000]);
  });
});
