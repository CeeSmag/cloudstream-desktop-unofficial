import { LanguageOption, Preferences } from '../types';

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
