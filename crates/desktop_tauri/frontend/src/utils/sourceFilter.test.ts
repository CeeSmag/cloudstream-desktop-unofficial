import { describe, expect, it } from 'vitest';
import { filterSourcesBySubtitles } from './sourceFilter';
import { Preferences, SourceInfo } from '../types';

const preferences: Preferences = {
  preferred_audio: 'eng',
  preferred_subtitles: ['eng', 'ara'],
  auto_subtitles: true,
  autoplay_next: true,
  skip_lookup: true,
  languages: [
    { code: 'eng', name: 'English', codes: ['eng', 'en'] },
    { code: 'ara', name: 'Arabic', codes: ['ara', 'ar'] },
  ],
};

const source = (index: number, languages: string[], selected = false): SourceInfo => ({
  index,
  label: `Source ${index}`,
  selected,
  checking: false,
  kind: 'MKV',
  host: 'h',
  url: 'u',
  size_bytes: null,
  subtitle_languages: languages,
});

const sources = [
  source(0, ['eng', 'ara']),
  source(1, ['English']),
  source(2, ['ar']),
  source(3, []),
];

const indexes = (list: SourceInfo[]) => list.map((item) => item.index);

describe('filterSourcesBySubtitles', () => {
  it('keeps every source when no language is toggled', () => {
    expect(indexes(filterSourcesBySubtitles(sources, preferences, []))).toEqual([0, 1, 2, 3]);
  });

  it('keeps every source until the preferences are known', () => {
    expect(indexes(filterSourcesBySubtitles(sources, null, ['eng']))).toEqual([0, 1, 2, 3]);
  });

  it('with English toggled, keeps the sources that have English subtitles', () => {
    expect(indexes(filterSourcesBySubtitles(sources, preferences, ['eng']))).toEqual([0, 1]);
  });

  it('with Arabic toggled, keeps the sources that have Arabic subtitles', () => {
    expect(indexes(filterSourcesBySubtitles(sources, preferences, ['ara']))).toEqual([0, 2]);
  });

  it('with English and Arabic toggled, keeps only the sources that have both', () => {
    expect(indexes(filterSourcesBySubtitles(sources, preferences, ['eng', 'ara']))).toEqual([0]);
  });

  it('keeps the source that is playing even when it does not match', () => {
    const playing = [source(0, ['eng', 'ara']), source(3, [], true)];
    expect(indexes(filterSourcesBySubtitles(playing, preferences, ['eng', 'ara']))).toEqual([0, 3]);
  });
});
