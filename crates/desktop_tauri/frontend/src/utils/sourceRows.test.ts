import { describe, expect, it } from 'vitest';
import { sourceRow } from './sourceRows';
import { Preferences, SourceInfo } from '../types';

const source = (over: Partial<SourceInfo>): SourceInfo => ({
  index: 3,
  label: '2160p - Spider-Man.Brand.New.Day.2026.2160p.WEB-DL.mkv',
  selected: false,
  checking: false,
  kind: 'MKV',
  host: 'cdn.example.com',
  url: 'https://cdn.example.com/file.mkv?sig=abc',
  size_bytes: 1_400_000_000,
  subtitle_languages: [],
  ...over,
});

describe('sourceRow', () => {
  it('keeps the full name, shows the kind as a tag and the size as a flag', () => {
    const row = sourceRow(source({}));
    expect(row.label).toContain('WEB-DL.mkv');
    expect(row.tags).toEqual(['MKV']);
    expect(row.badge).toBe('1.3 GB');
  });

  it('does not show the host or the address', () => {
    const row = sourceRow(source({}));
    expect(row.tags).not.toContain('cdn.example.com');
    expect(row.detail).toBeUndefined();
  });

  it('has no size flag when the size is unknown', () => {
    expect(sourceRow(source({ size_bytes: null })).badge).toBeUndefined();
  });

  it('marks a stream that is still being checked', () => {
    expect(sourceRow(source({ checking: true })).tags).toContain('checking…');
  });

  it('shows a flag under the source for each preferred subtitle language it offers, and none for the rest', () => {
    const preferences: Preferences = {
      preferred_audio: 'eng',
      preferred_subtitles: ['eng', 'ara', 'fre'],
      auto_subtitles: true,
      autoplay_next: true,
      skip_lookup: true,
      languages: [
        { code: 'eng', name: 'English', codes: ['eng', 'en'] },
        { code: 'ara', name: 'Arabic', codes: ['ara', 'ar'] },
        { code: 'fre', name: 'French', codes: ['fre', 'fra', 'fr'] },
      ],
    };
    const withBoth = sourceRow(source({ subtitle_languages: ['ara', 'eng'] }), preferences);
    expect(withBoth.flags?.map((flag) => flag.abbreviation)).toEqual(['En', 'Ar']);
    const onlyEnglish = sourceRow(source({ subtitle_languages: ['English'] }), preferences);
    expect(onlyEnglish.flags?.map((flag) => flag.abbreviation)).toEqual(['En']);
    expect(sourceRow(source({}), preferences).flags).toEqual([]);
    expect(sourceRow(source({ subtitle_languages: ['eng'] })).flags).toEqual([]);
  });
});
