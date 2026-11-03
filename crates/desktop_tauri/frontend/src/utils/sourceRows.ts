import { MenuOption } from '../components/OptionMenu';
import { Preferences, SourceInfo } from '../types';
import { formatFileSize } from './fileSize';
import { flagsForSource } from './subtitleFlags';

/**
 * The dropdown row of one stream: its full name, its kind, a flag for each preferred subtitle
 * language it offers, and a badge with the file size.
 */
export function sourceRow(source: SourceInfo, preferences: Preferences | null = null): MenuOption {
  const tags = source.kind === '' ? [] : [source.kind];
  if (source.checking) tags.push('checking…');
  const flags = preferences ? flagsForSource(source.subtitle_languages, preferences) : [];
  return {
    id: source.index,
    label: source.label,
    selected: source.selected,
    tags,
    flags: flags.map(({ code, flag, abbreviation, name }) => ({ key: code, flag, abbreviation, name })),
    badge: source.size_bytes === null ? undefined : formatFileSize(source.size_bytes),
  };
}
