import { describe, expect, it } from 'vitest';
import { skipPrompt } from './skipPrompt';
import { SkipSegment } from '../types';

const intro: SkipSegment = { kind: 'intro', start_ms: 10_000, end_ms: 60_000 };
const credits: SkipSegment = { kind: 'credits', start_ms: 1_400_000, end_ms: 1_430_000 };
const DURATION = 1_440_000;
const at = (positionMs: number, segments: SkipSegment[] = [intro, credits], hasNext = true) =>
  skipPrompt({ segments, positionMs, durationMs: DURATION, hasNext });

describe('skipPrompt', () => {
  it('offers to skip the intro while inside it and jumps to its end', () => {
    expect(at(30_000)).toEqual({ label: 'Skip Intro', action: 'seek', toMs: 60_000 });
  });

  it('includes the start and excludes the end of a segment', () => {
    expect(at(10_000)?.label).toBe('Skip Intro');
    expect(at(9_999)).toBeNull();
    expect(at(59_999)?.label).toBe('Skip Intro');
    expect(at(60_000)).toBeNull();
  });

  it('offers the next episode for credits that run to the end of the video', () => {
    expect(at(1_410_000)).toEqual({ label: 'Next episode', action: 'next' });
  });

  it('keeps Skip Credits when the credits end well before the end or there is no next episode', () => {
    const early: SkipSegment = { kind: 'credits', start_ms: 1_000_000, end_ms: 1_100_000 };
    expect(at(1_050_000, [early])).toEqual({ label: 'Skip Credits', action: 'seek', toMs: 1_100_000 });
    expect(at(1_410_000, [intro, credits], false)).toEqual({
      label: 'Skip Credits',
      action: 'seek',
      toMs: 1_430_000,
    });
  });

  it('treats credits ending exactly 20 s before the end as not the end of the episode', () => {
    const edge: SkipSegment = { kind: 'credits', start_ms: 1_000_000, end_ms: DURATION - 20_000 };
    expect(at(1_100_000, [edge])?.action).toBe('seek');
    const inside: SkipSegment = { kind: 'credits', start_ms: 1_000_000, end_ms: DURATION - 19_999 };
    expect(at(1_100_000, [inside])?.action).toBe('next');
  });

  it('without segments shows Next episode from 90 percent on, only when there is a next one', () => {
    expect(at(1_295_999, [])).toBeNull();
    expect(at(1_296_000, [])).toEqual({ label: 'Next episode', action: 'next' });
    expect(at(1_296_000, [], false)).toBeNull();
  });

  it('shows nothing before the video length is known', () => {
    expect(skipPrompt({ segments: [], positionMs: 5000, durationMs: 0, hasNext: true })).toBeNull();
  });

  it('labels recap and preview', () => {
    const recap: SkipSegment = { kind: 'recap', start_ms: 0, end_ms: 5000 };
    expect(at(1000, [recap])?.label).toBe('Skip Recap');
    const preview: SkipSegment = { kind: 'preview', start_ms: 100_000, end_ms: 200_000 };
    expect(at(150_000, [preview])?.label).toBe('Skip Preview');
  });
});
