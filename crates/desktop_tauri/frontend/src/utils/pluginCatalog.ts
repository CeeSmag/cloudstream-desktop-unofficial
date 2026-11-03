import { PluginHit } from '../types';
import { PluginView, pluginAction } from './extensionText';

/** What the Plugins tab narrows the list to. */
export interface CatalogFilter {
  text: string;
  view: PluginView;
  /** A language code, or an empty text for every language. */
  language: string;
}

/** The text the search looks through: name, description, authors, language and the repository. */
function searchable(hit: PluginHit): string {
  const { plugin } = hit;
  return [
    plugin.name,
    plugin.internal_name,
    plugin.description ?? '',
    plugin.authors.join(' '),
    plugin.language ?? '',
    hit.repository_name,
  ]
    .join(' ')
    .toLowerCase();
}

/** 0: the whole search is in the name; 1: every word is in the name; 2: every word is somewhere. */
function rankOf(hit: PluginHit, whole: string, words: string[]): number | null {
  const name = `${hit.plugin.name} ${hit.plugin.internal_name}`.toLowerCase();
  if (name.includes(whole)) return 0;
  if (words.every((word) => name.includes(word))) return 1;
  const everything = searchable(hit);
  return words.every((word) => everything.includes(word)) ? 2 : null;
}

function matchesView(hit: PluginHit, view: PluginView): boolean {
  const action = pluginAction(hit.plugin);
  if (view === 'available') return action === 'install';
  if (view === 'installed') return action !== 'install';
  if (view === 'updates') return action === 'update';
  return true;
}

/**
 * The extensions that fit the view, the language and the search words, best matches first (those
 * with the search in their name before those that only mention it); without a search the order is kept.
 */
export function filterHits(hits: PluginHit[], filter: CatalogFilter): PluginHit[] {
  const whole = filter.text.trim().toLowerCase();
  const words = whole.split(/\s+/).filter(Boolean);
  const chosen = filter.language.toLowerCase();
  const kept = hits.filter(
    (hit) => matchesView(hit, filter.view) && (chosen === '' || (hit.plugin.language ?? '').toLowerCase() === chosen),
  );
  if (words.length === 0) return kept;
  const ranked: { rank: number; hit: PluginHit }[] = [];
  for (const hit of kept) {
    const rank = rankOf(hit, whole, words);
    if (rank !== null) ranked.push({ rank, hit });
  }
  return ranked.sort((a, b) => a.rank - b.rank).map((entry) => entry.hit);
}

/** The languages the extensions are in, most common first, for the language filter. */
export function languageChoices(hits: PluginHit[]): { code: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const hit of hits) {
    const code = (hit.plugin.language ?? '').toLowerCase();
    if (code) counts.set(code, (counts.get(code) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([code, count]) => ({ code, count }))
    .sort((a, b) => b.count - a.count || a.code.localeCompare(b.code));
}
