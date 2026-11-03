import { LanguageOption, MediaTrack, Preferences } from '../types';

/** A subtitle language offered by the stream, as a flag with the first letters of its name. */
export interface SubtitleFlag {
  code: string;
  flag: string;
  /** The first two letters of the language's name, such as `En`. */
  abbreviation: string;
  name: string;
  /** The track to turn on for this language. */
  trackId: number;
  selected: boolean;
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

/** Whether a subtitle track is in `language`: by its code (`eng`, `en`, `en-US`), or by name when it has none. */
export function trackInLanguage(track: MediaTrack, language: LanguageOption): boolean {
  const code = (track.language ?? '').trim().toLowerCase().split('-')[0] ?? '';
  if (code && !['und', 'unk'].includes(code)) return language.codes.includes(code);
  return track.title.toLowerCase().includes(language.name.toLowerCase());
}

/**
 * The flags to show for the stream: one for each preferred subtitle language that the stream
 * offers, in the order the viewer chose them (the settings allow at most three).
 */
export function subtitleFlags(tracks: MediaTrack[], preferences: Preferences): SubtitleFlag[] {
  const subtitles = tracks.filter((track) => track.kind === 'subtitle');
  const flags: SubtitleFlag[] = [];
  for (const code of preferences.preferred_subtitles) {
    const language = preferences.languages.find((option) => option.code === code);
    const matching = language ? subtitles.filter((track) => trackInLanguage(track, language)) : [];
    const first = matching[0];
    if (!language || !first) continue;
    flags.push({
      code,
      flag: flagOfLanguage(language.name),
      abbreviation: abbreviationOfLanguage(language.name),
      name: language.name,
      trackId: (matching.find((track) => track.selected) ?? first).id,
      selected: matching.some((track) => track.selected),
    });
  }
  return flags;
}
