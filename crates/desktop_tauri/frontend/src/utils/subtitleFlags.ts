import { LanguageOption, MediaTrack, Preferences } from '../types';

/** A preferred subtitle language, as a flag with the first two letters of its name. */
export interface LanguageFlag {
  code: string;
  flag: string;
  /** The first two letters of the language's name, such as `En`. */
  abbreviation: string;
  name: string;
}

/** The country whose flag stands for each language the app offers. */
const COUNTRY_OF_LANGUAGE: Record<string, string> = {
  English: 'GB',
  Spanish: 'ES',
  French: 'FR',
  German: 'DE',
  Italian: 'IT',
  Portuguese: 'PT',
  Russian: 'RU',
  Japanese: 'JP',
  Korean: 'KR',
  Chinese: 'CN',
  Arabic: 'SA',
  Turkish: 'TR',
  Hindi: 'IN',
  Tamil: 'IN',
  Telugu: 'IN',
  Malayalam: 'IN',
  Kannada: 'IN',
  Bengali: 'BD',
  Marathi: 'IN',
  Punjabi: 'IN',
  Urdu: 'PK',
  Thai: 'TH',
  Vietnamese: 'VN',
  Indonesian: 'ID',
  Malay: 'MY',
  Filipino: 'PH',
  Polish: 'PL',
  Dutch: 'NL',
  Swedish: 'SE',
  Danish: 'DK',
  Norwegian: 'NO',
  Finnish: 'FI',
  Ukrainian: 'UA',
  Hebrew: 'IL',
  Greek: 'GR',
  Czech: 'CZ',
  Hungarian: 'HU',
  Romanian: 'RO',
  Bulgarian: 'BG',
  Croatian: 'HR',
  Serbian: 'RS',
  Persian: 'IR',
};

const FIRST_REGIONAL_INDICATOR = 0x1f1e6;
const LETTER_A = 65;
const ABBREVIATION_LENGTH = 2;

/** The flag emoji of a language, or a plain globe when no country is known for it. */
export function flagOfLanguage(name: string): string {
  const country = COUNTRY_OF_LANGUAGE[name];
  if (!country) return '🌐';
  return String.fromCodePoint(
    ...Array.from(country).map((letter) => FIRST_REGIONAL_INDICATOR + letter.charCodeAt(0) - LETTER_A),
  );
}

/** The first two letters of a language's name: `En`, `Sp`, `Fr`. */
export function abbreviationOfLanguage(name: string): string {
  return name.slice(0, ABBREVIATION_LENGTH);
}

/** Whether `value`, a subtitle language as a stream names it (`eng`, `en-US`, `Arabic`), is `language`. */
export function languageMatches(value: string, language: LanguageOption): boolean {
  const named = value.trim().toLowerCase();
  const code = named.split(/[-_]/)[0] ?? '';
  const name = language.name.toLowerCase();
  return language.codes.includes(named) || language.codes.includes(code) || named === name || named.startsWith(`${name} `);
}

/**
 * The flags to show under a source: one for each preferred subtitle language (the settings allow
 * at most three) that the source offers, in the order the viewer chose them.
 */
export function flagsForSource(languages: string[], preferences: Preferences): LanguageFlag[] {
  const flags: LanguageFlag[] = [];
  for (const code of preferences.preferred_subtitles) {
    const language = preferences.languages.find((option) => option.code === code);
    if (!language || !languages.some((value) => languageMatches(value, language))) continue;
    flags.push({
      code,
      flag: flagOfLanguage(language.name),
      abbreviation: abbreviationOfLanguage(language.name),
      name: language.name,
    });
  }
  return flags;
}

/** A flag as the dropdown rows draw it. */
export function menuFlags(flags: LanguageFlag[]): { key: string; flag: string; abbreviation: string; name: string }[] {
  return flags.map(({ code, flag, abbreviation, name }) => ({ key: code, flag, abbreviation, name }));
}

/** Whether a source offers subtitles in every one of the `wanted` languages (codes from the preferences). */
export function offersAllSubtitles(languages: string[], preferences: Preferences, wanted: string[]): boolean {
  return wanted.every((code) => {
    const language = preferences.languages.find((option) => option.code === code);
    return language !== undefined && languages.some((value) => languageMatches(value, language));
  });
}

/** How a subtitle track names its language: its code, or its title when it has no usable code. */
export function trackLanguageValues(track: MediaTrack): string[] {
  const code = (track.language ?? '').trim().toLowerCase();
  return code && code !== 'und' && code !== 'unk' ? [code] : [track.title];
}

/** The position of the track's language among the preferred subtitle languages; -1 when it is not one of them. */
export function preferredRankOfTrack(track: MediaTrack, preferences: Preferences): number {
  const values = trackLanguageValues(track);
  return preferences.preferred_subtitles.findIndex((code) => {
    const language = preferences.languages.find((option) => option.code === code);
    return language !== undefined && values.some((value) => languageMatches(value, language));
  });
}

/** The flags of the viewer's preferred subtitle languages, in the order they were chosen. */
export function preferredFlags(preferences: Preferences): LanguageFlag[] {
  return preferences.preferred_subtitles
    .map((_, rank) => flagOfPreferredRank(rank, preferences))
    .filter((flag): flag is LanguageFlag => flag !== null);
}

/** The flag of the preferred language at `rank` (a position in the preferred subtitle languages). */
export function flagOfPreferredRank(rank: number, preferences: Preferences): LanguageFlag | null {
  const code = preferences.preferred_subtitles[rank];
  const language = preferences.languages.find((option) => option.code === code);
  if (!code || !language) return null;
  return {
    code,
    flag: flagOfLanguage(language.name),
    abbreviation: abbreviationOfLanguage(language.name),
    name: language.name,
  };
}
