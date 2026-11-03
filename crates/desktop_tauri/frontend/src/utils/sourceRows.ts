import { MenuOption } from '../components/OptionMenu';
import { SourceInfo } from '../types';
import { formatFileSize } from './fileSize';

/** The dropdown row of one stream: its full name, its kind, and a flag with the file size. */
export function sourceRow(source: SourceInfo): MenuOption {
  const tags = source.kind === '' ? [] : [source.kind];
  if (source.checking) tags.push('checking…');
  return {
    id: source.index,
    label: source.label,
    selected: source.selected,
    tags,
    badge: source.size_bytes === null ? undefined : formatFileSize(source.size_bytes),
  };
}
