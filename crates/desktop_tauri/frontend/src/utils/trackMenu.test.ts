import { describe, expect, it } from 'vitest';
import { trackOptions } from './trackMenu';
import { MediaTrack, Preferences } from '../types';

const preferences: Preferences = {
  preferred_audio: 'eng',
  preferred_subtitles: ['eng', 'ara'],
  auto_subtitles: true,
  autoplay_next: true,
  skip_lookup: true,
  languages: [
    { code: 'eng', name: 'English', codes: ['eng', 'en'] },
    { code: 'ara', name: 'Arabic', codes: ['ara', 'ar'] },
    { code: 'fre', name: 'French', codes: ['fre', 'fra', 'fr'] },
  ],
};

const subtitle = (id: number, language: string | null, title: string, selected = false): MediaTrack => ({
  id,
  kind: 'subtitle',
  language,
  title,
  selected,
});

const ids = (tracks: MediaTrack[], prefs: Preferences | null) =>
  trackOptions(tracks, 'subtitle', prefs).map((option) => option.id);

describe('trackOptions', () => {
  const tracks = [
    subtitle(1, 'fre', 'French'),
    subtitle(2, 'ara', 'Arabic'),
    subtitle(3, 'ger', 'German'),
    subtitle(4, 'eng', 'English'),
    subtitle(5, 'en', 'English'),
  ];

  it('puts the preferred subtitles first in the order they were chosen, and the rest after them', () => {
    expect(ids(tracks, preferences)).toEqual([4, 5, 2, 1, 3]);
  });

  it('keeps the original order without preferences', () => {
    expect(ids(tracks, null)).toEqual([1, 2, 3, 4, 5]);
  });

  it('marks only the preferred rows with a flag', () => {
    const options = trackOptions(tracks, 'subtitle', preferences);
    expect(options.map((option) => option.flags?.[0]?.abbreviation)).toEqual(['En', 'En', 'Ar', undefined, undefined]);
  });

  it('recognises a track by its title when it has no language code', () => {
    const untagged = [subtitle(1, null, 'French'), subtitle(2, 'und', 'Arabic (forced)')];
    expect(ids(untagged, preferences)).toEqual([2, 1]);
  });

  it('numbers tracks with the same name and keeps the selection', () => {
    const options = trackOptions([subtitle(1, 'eng', 'English'), subtitle(2, 'eng', 'English', true)], 'subtitle', null);
    expect(options.map((option) => option.label)).toEqual(['English 1', 'English 2']);
    expect(options.map((option) => option.selected)).toEqual([false, true]);
  });

  it('does not reorder audio tracks', () => {
    const audio: MediaTrack[] = [
      { ...subtitle(1, 'fre', 'French'), kind: 'audio' },
      { ...subtitle(2, 'eng', 'English'), kind: 'audio' },
    ];
    expect(trackOptions(audio, 'audio', preferences).map((option) => option.id)).toEqual([1, 2]);
  });
});
