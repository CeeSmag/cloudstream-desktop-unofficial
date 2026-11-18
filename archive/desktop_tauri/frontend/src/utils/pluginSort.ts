import { PluginHit, RepositoryPlugin } from '../types';

/** How the Plugins list can be ordered. */
export type PluginSort = 'language' | 'name' | 'size' | 'date';

/** Rank of a language among preferred codes (0 = first preferred); missing / unmatched → large. */
function languageRank(code: string | null, preferred: string[]): number {
  if (!code) return Number.MAX_SAFE_INTEGER - 1;
  const lower = code.toLowerCase();
  const index = preferred.findIndex((entry) => entry.toLowerCase() === lower);
  return index === -1 ? Number.MAX_SAFE_INTEGER - 2 : index;
}

function byName(a: RepositoryPlugin, b: RepositoryPlugin): number {
  return a.name.localeCompare(b.name) || a.internal_name.localeCompare(b.internal_name);
}

function comparePlugins(a: RepositoryPlugin, b: RepositoryPlugin, sort: PluginSort, preferred: string[]): number {
  if (sort === 'name') return byName(a, b);
  if (sort === 'size') {
    const left = a.file_size;
    const right = b.file_size;
    if (left === null && right === null) return byName(a, b);
    if (left === null) return 1;
    if (right === null) return -1;
    return right - left || byName(a, b);
  }
  if (sort === 'date') {
    const left = a.installed_at_ms;
    const right = b.installed_at_ms;
    if (left === null && right === null) return byName(a, b);
    if (left === null) return 1;
    if (right === null) return -1;
    // 0 means unknown (pre-migration); treat as older than any real timestamp.
    const leftKey = left === 0 ? Number.MIN_SAFE_INTEGER : left;
    const rightKey = right === 0 ? Number.MIN_SAFE_INTEGER : right;
    return rightKey - leftKey || byName(a, b);
  }
  const rank = languageRank(a.language, preferred) - languageRank(b.language, preferred);
  if (rank !== 0) return rank;
  const left = (a.language ?? '').toLowerCase();
  const right = (b.language ?? '').toLowerCase();
  return left.localeCompare(right) || byName(a, b);
}

/** Copies and sorts plugins by the chosen key; preferred subtitle codes lead language order. */
export function sortPlugins(
  plugins: RepositoryPlugin[],
  sort: PluginSort,
  preferredSubtitles: string[] = [],
): RepositoryPlugin[] {
  return [...plugins].sort((a, b) => comparePlugins(a, b, sort, preferredSubtitles));
}

/** Same ordering as `sortPlugins`, for catalog hits that wrap a plugin. */
export function sortHits(
  hits: PluginHit[],
  sort: PluginSort,
  preferredSubtitles: string[] = [],
): PluginHit[] {
  return [...hits].sort((a, b) => comparePlugins(a.plugin, b.plugin, sort, preferredSubtitles));
}
