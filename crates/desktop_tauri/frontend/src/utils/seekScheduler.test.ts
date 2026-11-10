import { describe, expect, it } from 'vitest';
import { createSeekScheduler, FAST_GAP_MS, SLOW_GAP_MS, SUSTAINED_MS, Timers } from './seekScheduler';

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
  return { timers, advance, now: () => time, waiting: () => pending.size };
}

function rig() {
  const sent: { target: number; at: number }[] = [];
  const clock = fakeTimers();
  const scheduler = createSeekScheduler((ms) => sent.push({ target: ms, at: clock.now() }), clock.timers);
  return { sent, targets: () => sent.map((jump) => jump.target), scheduler, ...clock };
}

describe('createSeekScheduler', () => {
  it('sends a single press at once, with no wait', () => {
    const { sent, scheduler, advance, waiting } = rig();
    advance(10_000);
    scheduler.press(7000);
    expect(sent).toEqual([{ target: 7000, at: 10_000 }]);
    expect(waiting()).toBe(0);
    advance(5000);
    expect(sent.length).toBe(1);
  });

  it('three quick taps: the first goes at once, the rest follow as one jump to the last target', () => {
    const { sent, scheduler, advance } = rig();
    scheduler.press(5000);
    advance(80);
    scheduler.press(10_000);
    advance(80);
    scheduler.press(15_000);
    expect(sent).toEqual([{ target: 5000, at: 0 }]);
    advance(FAST_GAP_MS);
    expect(sent).toEqual([
      { target: 5000, at: 0 },
      { target: 15_000, at: FAST_GAP_MS },
    ]);
  });

  it('taps slower than the gap are each sent at once', () => {
    const { sent, scheduler, advance } = rig();
    for (let n = 1; n <= 4; n++) {
      scheduler.press(n * 5000);
      advance(FAST_GAP_MS + 50);
    }
    expect(sent.map((jump) => jump.at)).toEqual([0, 250, 500, 750]);
    expect(sent.map((jump) => jump.target)).toEqual([5000, 10_000, 15_000, 20_000]);
  });

  it('never sends two jumps closer together than the gap', () => {
    const { sent, scheduler, advance } = rig();
    for (let n = 1; n <= 40; n++) {
      scheduler.press(n * 5000);
      advance(33);
    }
    advance(1000);
    const gaps = sent.slice(1).map((jump, index) => jump.at - sent[index].at);
    expect(Math.min(...gaps)).toBeGreaterThanOrEqual(FAST_GAP_MS);
    expect(sent[sent.length - 1].target).toBe(200_000);
  });

  it('a held key (a press every 33 ms for 10 s) slows to one jump every half second, never a flood', () => {
    const { sent, targets, scheduler, advance } = rig();
    let target = 0;
    for (let n = 0; n < 300; n++) {
      target += 5000;
      scheduler.press(target);
      advance(33);
    }
    advance(1000);
    // About 5 in the first second, then 2 a second.
    expect(sent.length).toBeLessThanOrEqual(26);
    expect(sent.length).toBeGreaterThanOrEqual(18);
    const late = sent.filter((jump) => jump.at > SUSTAINED_MS + SLOW_GAP_MS);
    const lateGaps = late.slice(1).map((jump, index) => jump.at - late[index].at);
    expect(Math.min(...lateGaps)).toBeGreaterThanOrEqual(SLOW_GAP_MS);
    expect(targets()[sent.length - 1]).toBe(target);
    expect([...targets()].sort((a, b) => a - b)).toEqual(targets());
  });

  it('after a pause the next press is fast again', () => {
    const { sent, scheduler, advance, now } = rig();
    for (let n = 0; n < 60; n++) {
      scheduler.press(n);
      advance(33);
    }
    advance(2000);
    const before = sent.length;
    scheduler.press(999);
    expect(sent[before]).toEqual({ target: 999, at: now() });
    advance(50);
    scheduler.press(1000);
    advance(FAST_GAP_MS);
    expect(sent[before + 1].at - sent[before].at).toBe(FAST_GAP_MS);
  });

  it('sendNow sends at once and drops a jump that was waiting', () => {
    const { targets, scheduler, advance, waiting } = rig();
    scheduler.press(5000);
    scheduler.press(10_000);
    expect(waiting()).toBe(1);
    scheduler.sendNow(90_000);
    expect(targets()).toEqual([5000, 90_000]);
    expect(waiting()).toBe(0);
    advance(5000);
    expect(targets()).toEqual([5000, 90_000]);
  });

  it('cancel forgets a waiting jump without sending it', () => {
    const { targets, scheduler, advance, waiting } = rig();
    scheduler.press(5000);
    scheduler.press(10_000);
    scheduler.cancel();
    advance(5000);
    expect(targets()).toEqual([5000]);
    expect(waiting()).toBe(0);
  });
});
