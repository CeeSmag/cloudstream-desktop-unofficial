import { describe, expect, it } from 'vitest';
import {
  abbreviationOfLanguage,
  flagOfLanguage,
  subtitleFlags,
  trackInLanguage,
} from './subtitleFlags';
import { LanguageOption, MediaTrack, Preferences } from '../types';

const english: LanguageOption = { code: 'eng', name: 'English', codes: ['eng', 'en'] };
const spanish: LanguageOption = { code: 'spa', name: 'Spanish', codes: ['spa', 'es'] };
const french: LanguageOption = { code: 'fre', name: 'French', codes: ['fre', 'fra', 'fr'] };

const track = (id: number, language: string | null, title: string, selected = false): MediaTrack => ({
  id,
  kind: 'subtitle',
  language,
  title,
  selected,
});

const preferences = (preferred: string[]): Preferences => ({
  preferred_audio: 'eng',
  preferred_subtitles: preferred,
  languages: [english, spanish, french],
});

describe('subtitle flags', () => {
  it('draws a flag from the country of the language and abbreviates the name to two letters', () => {
    expect(flagOfLanguage('English')).toBe('🇬🇧');
    expect(flagOfLanguage('Spanish')).toBe('🇪🇸');
    expect(flagOfLanguage('Klingon')).toBe('🌐');
    expect(abbreviationOfLanguage('English')).toBe('En');
    expect(abbreviationOfLanguage('Spanish')).toBe('Sp');
  });

  it('recognises a track by any code of the language, with or without a region', () => {
    expect(trackInLanguage(track(1, 'eng', ''), english)).toBe(true);
    expect(trackInLanguage(track(1, 'en-US', ''), english)).toBe(true);
    expect(trackInLanguage(track(1, 'spa', ''), english)).toBe(false);
  });

  it('falls back to the name in the title when the track has no usable language', () => {
    expect(trackInLanguage(track(1, null, 'English (SDH)'), english)).toBe(true);
    expect(trackInLanguage(track(1, 'und', 'Spanish'), english)).toBe(false);
  });

  it('shows only the preferred languages the stream offers, in the viewer’s order', () => {
    const tracks = [track(1, 'spa', ''), track(2, 'fre', ''), track(3, 'eng', '')];
    const flags = subtitleFlags(tracks, preferences(['eng', 'spa']));
    expect(flags.map((flag) => flag.abbreviation)).toEqual(['En', 'Sp']);
    expect(flags.map((flag) => flag.trackId)).toEqual([3, 1]);
  });

  it('shows nothing when the stream has none of the preferred languages', () => {
    expect(subtitleFlags([track(1, 'fre', '')], preferences(['eng']))).toEqual([]);
  });

  it('marks the flag whose track is showing and points at that track', () => {
    const tracks = [track(1, 'eng', '', false), track(2, 'eng', '', true)];
    const [flag] = subtitleFlags(tracks, preferences(['eng']));
    expect(flag?.selected).toBe(true);
    expect(flag?.trackId).toBe(2);
  });

  it('ignores audio tracks', () => {
    const audio: MediaTrack = { ...track(1, 'eng', ''), kind: 'audio' };
    expect(subtitleFlags([audio], preferences(['eng']))).toEqual([]);
  });
});
