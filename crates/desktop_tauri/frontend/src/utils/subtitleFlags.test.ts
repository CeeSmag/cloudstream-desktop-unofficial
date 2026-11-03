import { describe, expect, it } from 'vitest';
import { abbreviationOfLanguage, flagOfLanguage, flagsForSource, languageMatches } from './subtitleFlags';
import { LanguageOption, Preferences } from '../types';

const english: LanguageOption = { code: 'eng', name: 'English', codes: ['eng', 'en'] };
const arabic: LanguageOption = { code: 'ara', name: 'Arabic', codes: ['ara', 'ar'] };
const french: LanguageOption = { code: 'fre', name: 'French', codes: ['fre', 'fra', 'fr'] };

const preferences = (preferred: string[]): Preferences => ({
  preferred_audio: 'eng',
  preferred_subtitles: preferred,
  languages: [english, arabic, french],
});

describe('subtitle flags', () => {
  it('draws a flag from the country of the language and abbreviates the name to two letters', () => {
    expect(flagOfLanguage('English')).toBe('🇬🇧');
    expect(flagOfLanguage('Arabic')).toBe('🇸🇦');
    expect(flagOfLanguage('Klingon')).toBe('🌐');
    expect(abbreviationOfLanguage('English')).toBe('En');
    expect(abbreviationOfLanguage('Arabic')).toBe('Ar');
  });

  it('recognises a language by any of its codes, with a region, or by its name', () => {
    expect(languageMatches('eng', english)).toBe(true);
    expect(languageMatches('en-US', english)).toBe(true);
    expect(languageMatches('ENGLISH', english)).toBe(true);
    expect(languageMatches('English (SDH)', english)).toBe(true);
    expect(languageMatches('ara', english)).toBe(false);
    expect(languageMatches('Arabic', english)).toBe(false);
  });

  it('shows only the preferred languages the source offers, in the viewer’s order', () => {
    const flags = flagsForSource(['fre', 'ar', 'eng'], preferences(['eng', 'ara']));
    expect(flags.map((flag) => flag.abbreviation)).toEqual(['En', 'Ar']);
  });

  it('shows no flag for a language the source does not have', () => {
    const flags = flagsForSource(['eng'], preferences(['eng', 'ara', 'fre']));
    expect(flags.map((flag) => flag.name)).toEqual(['English']);
  });

  it('shows nothing when the source offers none of the preferred languages', () => {
    expect(flagsForSource(['fre'], preferences(['eng']))).toEqual([]);
    expect(flagsForSource([], preferences(['eng']))).toEqual([]);
  });
});
