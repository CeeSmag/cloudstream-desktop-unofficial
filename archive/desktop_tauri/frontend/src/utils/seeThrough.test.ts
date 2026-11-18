import { describe, expect, it } from 'vitest';
import {
  IDLE_PAINT,
  isSeeThrough,
  PAINT_PROOF_MS,
  paintStep,
  PaintEvent,
  PaintState,
  videoHasPainted,
} from './seeThrough';

const run = (events: PaintEvent[], from: PaintState = IDLE_PAINT): PaintState =>
  events.reduce(paintStep, from);

const start = (positionMs: number): PaintEvent => ({ type: 'activity', active: true, positionMs });
const stop: PaintEvent = { type: 'activity', active: false, positionMs: 0 };
const at = (positionMs: number): PaintEvent => ({ type: 'position', positionMs });

describe('videoHasPainted', () => {
  it('is false until the position has moved enough', () => {
    expect(videoHasPainted(5_000, 5_000)).toBe(false);
    expect(videoHasPainted(5_000, 5_000 + PAINT_PROOF_MS - 1)).toBe(false);
  });

  it('is true once the position moved by the proof distance', () => {
    expect(videoHasPainted(5_000, 5_000 + PAINT_PROOF_MS)).toBe(true);
  });

  it('is false when the position went backwards (a seek before the first picture)', () => {
    expect(videoHasPainted(5_000, 1_000)).toBe(false);
  });
});

describe('paintStep', () => {
  it('stays opaque when playback starts and until the position moves', () => {
    const state = run([start(5_000), at(5_000), at(5_000 + PAINT_PROOF_MS - 1)]);
    expect(state.active).toBe(true);
    expect(isSeeThrough(state)).toBe(false);
  });

  it('turns see-through once the position has moved enough', () => {
    expect(isSeeThrough(run([start(5_000), at(5_000 + PAINT_PROOF_MS)]))).toBe(true);
  });

  it('turns see-through on the timeout when the video starts paused and never moves', () => {
    const started = run([start(5_000), at(5_000)]);
    expect(isSeeThrough(paintStep(started, { type: 'timeout', generation: started.generation }))).toBe(true);
  });

  it('ignores position and timeout events while nothing plays', () => {
    expect(run([at(9_999), { type: 'timeout', generation: 0 }])).toEqual(IDLE_PAINT);
  });

  it('goes back to opaque when playback stops, and starts opaque again next time', () => {
    const painted = run([start(5_000), at(6_000), stop]);
    expect(isSeeThrough(painted)).toBe(false);
    const again = run([start(0)], painted);
    expect(again.active).toBe(true);
    expect(isSeeThrough(again)).toBe(false);
    expect(again.startMs).toBe(0);
  });

  it('ignores the timeout of an earlier playback that arrives after a new one started', () => {
    const first = run([start(1_000)]);
    const second = run([stop, start(2_000)], first);
    expect(second.generation).toBe(first.generation + 1);
    expect(isSeeThrough(paintStep(second, { type: 'timeout', generation: first.generation }))).toBe(false);
    expect(isSeeThrough(paintStep(second, { type: 'timeout', generation: second.generation }))).toBe(true);
  });

  it('does not restart the wait when activity repeats while already active', () => {
    const state = run([start(1_000), at(1_100)]);
    expect(run([start(5_000)], state)).toEqual(state);
  });

  it('stays painted when the position later moves back (a seek after the first picture)', () => {
    expect(isSeeThrough(run([start(5_000), at(6_000), at(100)]))).toBe(true);
  });
});
