import { describe, expect, it } from 'vitest';
import { mergeSkipSegments, sameSegments, segmentsFromChapters } from './chapterSkips';
import { Chapter, SkipSegment } from '../types';

const DURATION = 1_440_000;
const chapter = (title: string, time_ms: number): Chapter => ({ title, time_ms });

describe('segmentsFromChapters', () => {
  it('turns an opening chapter into an intro that ends where the next chapter starts', () => {
    const chapters = [chapter('Opening', 0), chapter('Part A', 90_000), chapter('Part B', 700_000)];
    expect(segmentsFromChapters(chapters, DURATION)).toEqual([{ kind: 'intro', start_ms: 0, end_ms: 90_000 }]);
  });

  it('finds intro, recap, credits and preview in one episode', () => {
    const chapters = [
      chapter('Recap', 0),
      chapter('Opening', 60_000),
      chapter('Part A', 150_000),
      chapter('Ending', 1_320_000),
      chapter('Next Time', 1_400_000),
    ];
    expect(segmentsFromChapters(chapters, DURATION)).toEqual([
      { kind: 'recap', start_ms: 0, end_ms: 60_000 },
      { kind: 'intro', start_ms: 60_000, end_ms: 150_000 },
      { kind: 'credits', start_ms: 1_320_000, end_ms: 1_400_000 },
      { kind: 'preview', start_ms: 1_400_000, end_ms: DURATION },
    ]);
  });

  it('lets the last chapter run to the end of the video', () => {
    const chapters = [chapter('Part A', 0), chapter('Credits', 1_380_000)];
    expect(segmentsFromChapters(chapters, DURATION)).toEqual([
      { kind: 'credits', start_ms: 1_380_000, end_ms: DURATION },
    ]);
  });

  it('cannot tell where the last chapter ends while the length of the video is unknown', () => {
    const chapters = [chapter('Opening', 0), chapter('Part A', 90_000), chapter('Ending', 1_380_000)];
    expect(segmentsFromChapters(chapters, 0)).toEqual([{ kind: 'intro', start_ms: 0, end_ms: 90_000 }]);
  });

  it('matches whole words in any case, with a number after the word', () => {
    const titles: Array<[string, SkipSegment['kind']]> = [
      ['OPENING', 'intro'],
      ['Intro', 'intro'],
      ['Opening 2', 'intro'],
      ['OP', 'intro'],
      ['op1', 'intro'],
      ['ED', 'credits'],
      ['Ed 2', 'credits'],
      ['End Credits', 'credits'],
      ['Outro', 'credits'],
      ['Previously on', 'recap'],
      ['Preview', 'preview'],
      ['Next time', 'preview'],
    ];
    for (const [title, kind] of titles) {
      const found = segmentsFromChapters([chapter(title, 0), chapter('Part A', 60_000)], DURATION);
      expect(found, title).toEqual([{ kind, start_ms: 0, end_ms: 60_000 }]);
    }
  });

  it('does not take a word that only contains a keyword, nor a name that is spelled like one', () => {
    const titles = ['Chapter 1', 'Operation', 'Open Water', 'Ed and Jo meet', 'Introducing Bob', 'Spending'];
    for (const title of titles) {
      const found = segmentsFromChapters([chapter(title, 0), chapter('Part A', 60_000)], DURATION);
      expect(found, title).toEqual([]);
    }
  });

  it('lets the first keyword of a title decide', () => {
    const found = segmentsFromChapters([chapter('Opening Credits', 0), chapter('Part A', 60_000)], DURATION);
    expect(found).toEqual([{ kind: 'intro', start_ms: 0, end_ms: 60_000 }]);
  });

  it('ignores chapters that are too short to be a skippable part', () => {
    const chapters = [chapter('Opening', 0), chapter('Part A', 2_000)];
    expect(segmentsFromChapters(chapters, DURATION)).toEqual([]);
  });

  it('joins two neighbouring chapters of the same kind into one segment', () => {
    const chapters = [chapter('Credits', 1_300_000), chapter('Credits 2', 1_350_000), chapter('Next time', 1_400_000)];
    expect(segmentsFromChapters(chapters, DURATION)).toEqual([
      { kind: 'credits', start_ms: 1_300_000, end_ms: 1_400_000 },
      { kind: 'preview', start_ms: 1_400_000, end_ms: DURATION },
    ]);
  });

  it('copes with chapters out of order and with a stream that has none', () => {
    const chapters = [chapter('Part A', 90_000), chapter('Opening', 0)];
    expect(segmentsFromChapters(chapters, DURATION)).toEqual([{ kind: 'intro', start_ms: 0, end_ms: 90_000 }]);
    expect(segmentsFromChapters([], DURATION)).toEqual([]);
  });
});

describe('sameSegments', () => {
  const intro: SkipSegment = { kind: 'intro', start_ms: 0, end_ms: 90_000 };

  it('is true for lists with the same parts, even when they are different objects', () => {
    expect(sameSegments([intro], [{ ...intro }])).toBe(true);
    expect(sameSegments([], [])).toBe(true);
  });

  it('is false when the length, a kind or a time differs', () => {
    expect(sameSegments([intro], [])).toBe(false);
    expect(sameSegments([intro], [{ ...intro, kind: 'recap' }])).toBe(false);
    expect(sameSegments([intro], [{ ...intro, start_ms: 1 }])).toBe(false);
    expect(sameSegments([intro], [{ ...intro, end_ms: 1 }])).toBe(false);
  });
});

describe('mergeSkipSegments', () => {
  const chapterIntro: SkipSegment = { kind: 'intro', start_ms: 0, end_ms: 90_000 };
  const onlineIntro: SkipSegment = { kind: 'intro', start_ms: 5_000, end_ms: 80_000 };
  const onlineCredits: SkipSegment = { kind: 'credits', start_ms: 1_380_000, end_ms: 1_440_000 };

  it('uses the online segments when the file has no chapters that mean anything', () => {
    expect(mergeSkipSegments([], [onlineIntro, onlineCredits])).toEqual([onlineIntro, onlineCredits]);
  });

  it('prefers the chapters for a kind they cover and keeps the online times for the others', () => {
    expect(mergeSkipSegments([chapterIntro], [onlineIntro, onlineCredits])).toEqual([chapterIntro, onlineCredits]);
  });

  it('returns the chapter segments alone when nothing is known online', () => {
    expect(mergeSkipSegments([chapterIntro], [])).toEqual([chapterIntro]);
  });

  it('keeps the result in time order', () => {
    const credits: SkipSegment = { kind: 'credits', start_ms: 1_380_000, end_ms: 1_440_000 };
    expect(mergeSkipSegments([credits], [onlineIntro])).toEqual([onlineIntro, credits]);
  });
});
