import { describe, expect, it } from 'vitest';
import { chapterAt, chapterNotches, hoverInfo, segmentAt, segmentMarks, TOOLTIP_EDGE_PERCENT } from './seekBarHover';
import { Chapter, SkipSegment } from '../types';

const intro: SkipSegment = { kind: 'intro', start_ms: 10_000, end_ms: 60_000 };
const credits: SkipSegment = { kind: 'credits', start_ms: 900_000, end_ms: 1_000_000 };
const bar = { left: 100, width: 1000, durationMs: 1_000_000, segments: [intro, credits] };

describe('hoverInfo', () => {
  it('gives the time under the pointer and where to put the tooltip', () => {
    const info = hoverInfo({ ...bar, clientX: 600 });
    expect(info).toEqual({ positionMs: 500_000, percent: 50, label: null, chapter: null });
  });

  it('names the segment under the pointer, counting its start but not its end', () => {
    expect(hoverInfo({ ...bar, clientX: 100 + 10 })?.label).toBe('Intro');
    expect(hoverInfo({ ...bar, clientX: 100 + 9 })?.label).toBeNull();
    expect(hoverInfo({ ...bar, clientX: 100 + 59 })?.label).toBe('Intro');
    expect(hoverInfo({ ...bar, clientX: 100 + 60 })?.label).toBeNull();
    expect(hoverInfo({ ...bar, clientX: 100 + 950 })?.label).toBe('Credits');
  });

  it('keeps the tooltip off the ends but the time exact at both ends and beyond', () => {
    const start = hoverInfo({ ...bar, clientX: 100 });
    expect(start?.positionMs).toBe(0);
    expect(start?.percent).toBe(TOOLTIP_EDGE_PERCENT);
    const end = hoverInfo({ ...bar, clientX: 1100 });
    expect(end?.positionMs).toBe(1_000_000);
    expect(end?.percent).toBe(100 - TOOLTIP_EDGE_PERCENT);
    expect(hoverInfo({ ...bar, clientX: 5000 })?.positionMs).toBe(1_000_000);
    expect(hoverInfo({ ...bar, clientX: -50 })?.positionMs).toBe(0);
  });

  it('shows nothing while the length of the video or the bar is unknown', () => {
    expect(hoverInfo({ ...bar, clientX: 600, durationMs: 0 })).toBeNull();
    expect(hoverInfo({ ...bar, clientX: 600, width: 0 })).toBeNull();
  });
});

describe('segmentAt', () => {
  it('finds the segment that contains a position', () => {
    expect(segmentAt([intro, credits], 30_000)).toBe(intro);
    expect(segmentAt([intro, credits], 500_000)).toBeNull();
    expect(segmentAt([], 1)).toBeNull();
  });
});

describe('segmentMarks', () => {
  it('places each segment along the bar in percent', () => {
    expect(segmentMarks([intro, credits], 1_000_000)).toEqual([
      { kind: 'intro', left: 1, width: 5 },
      { kind: 'credits', left: 90, width: 10 },
    ]);
  });

  it('cuts a segment that runs past the end and leaves out empty or out-of-video ones', () => {
    const past: SkipSegment = { kind: 'credits', start_ms: 950_000, end_ms: 5_000_000 };
    const outside: SkipSegment = { kind: 'recap', start_ms: 2_000_000, end_ms: 3_000_000 };
    const empty: SkipSegment = { kind: 'preview', start_ms: 5000, end_ms: 5000 };
    const backwards: SkipSegment = { kind: 'preview', start_ms: 9000, end_ms: 100 };
    expect(segmentMarks([past, outside, empty, backwards], 1_000_000)).toEqual([
      { kind: 'credits', left: 95, width: 5 },
    ]);
  });

  it('draws nothing before the length is known', () => {
    expect(segmentMarks([intro], 0)).toEqual([]);
  });
});

const chapters: Chapter[] = [
  { title: 'Opening', time_ms: 0 },
  { title: 'Part A', time_ms: 100_000 },
  { title: 'Part B', time_ms: 600_000 },
];

describe('chapterAt', () => {
  it('is the last chapter that has started, counting its start exactly', () => {
    expect(chapterAt(chapters, 0)?.title).toBe('Opening');
    expect(chapterAt(chapters, 99_999)?.title).toBe('Opening');
    expect(chapterAt(chapters, 100_000)?.title).toBe('Part A');
    expect(chapterAt(chapters, 900_000)?.title).toBe('Part B');
  });

  it('is nothing before the first chapter starts or without chapters', () => {
    expect(chapterAt([{ title: 'Late', time_ms: 5000 }], 1000)).toBeNull();
    expect(chapterAt([], 1000)).toBeNull();
  });
});

describe('chapterNotches', () => {
  it('puts a divider where each chapter after the first starts', () => {
    expect(chapterNotches(chapters, 1_000_000)).toEqual([10, 60]);
  });

  it('draws none for a single chapter, no chapters or an unknown length', () => {
    expect(chapterNotches([chapters[0]], 1_000_000)).toEqual([]);
    expect(chapterNotches([], 1_000_000)).toEqual([]);
    expect(chapterNotches(chapters, 0)).toEqual([]);
  });

  it('leaves out chapters at the very start or at or past the end, and repeats', () => {
    const odd: Chapter[] = [
      { title: 'a', time_ms: 0 },
      { title: 'b', time_ms: 250_000 },
      { title: 'b again', time_ms: 250_000 },
      { title: 'end', time_ms: 1_000_000 },
      { title: 'beyond', time_ms: 2_000_000 },
    ];
    expect(chapterNotches(odd, 1_000_000)).toEqual([25]);
  });
});

describe('hoverInfo with chapters', () => {
  it('names the chapter under the pointer next to the segment', () => {
    const info = hoverInfo({ ...bar, chapters, clientX: 100 + 30 });
    expect(info?.chapter).toBe('Opening');
    expect(info?.label).toBe('Intro');
    expect(hoverInfo({ ...bar, chapters, clientX: 100 + 700 })?.chapter).toBe('Part B');
  });

  it('has no chapter when the stream has none', () => {
    expect(hoverInfo({ ...bar, clientX: 600 })?.chapter).toBeNull();
  });
});
