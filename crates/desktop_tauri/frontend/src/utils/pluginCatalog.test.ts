import { describe, expect, it } from 'vitest';
import { PluginHit, RepositoryPlugin } from '../types';
import { filterHits, languageChoices } from './pluginCatalog';

const hit = (
  name: string,
  repository: string,
  over: Partial<RepositoryPlugin> = {},
): PluginHit => ({
  repository_url: `https://${repository}/repo.json`,
  repository_name: repository,
  repository_icon_url: null,
  plugin: {
    internal_name: name.replace(/\s/g, ''),
    name,
    version: 1,
    description: null,
    authors: [],
    language: null,
    icon_url: null,
    file_size: null,
    installed_version: null,
    ...over,
  },
});

const all = [
  hit('Movie Hub', 'Alpha', { description: 'Movies and series', authors: ['Dana'], language: 'en' }),
  hit('Anime Land', 'Alpha', { description: 'Watch anime online', language: 'ja', installed_version: 1 }),
  hit('Hub Anime', 'Beta', { description: 'Anime from many sites', language: 'en', installed_version: 0 }),
  hit('Cee', 'Beta', { description: 'Iraqi TV', authors: ['Abod'], language: 'ar' }),
];

const none = { text: '', view: 'all' as const, language: '' };
const names = (hits: PluginHit[]) => hits.map((h) => h.plugin.name);

describe('filterHits', () => {
  it('keeps everything and the order when nothing is asked', () => {
    expect(names(filterHits(all, none))).toEqual(['Movie Hub', 'Anime Land', 'Hub Anime', 'Cee']);
  });

  it('puts name matches before description matches', () => {
    expect(names(filterHits(all, { ...none, text: 'hub' }))).toEqual(['Movie Hub', 'Hub Anime']);
    expect(names(filterHits(all, { ...none, text: 'series' }))).toEqual(['Movie Hub']);
  });

  it('needs every word, in any order, anywhere', () => {
    expect(names(filterHits(all, { ...none, text: 'hub movies' }))).toEqual(['Movie Hub']);
    expect(filterHits(all, { ...none, text: 'hub nothing' })).toHaveLength(0);
  });

  it('searches authors, languages and repository names, in any case', () => {
    expect(names(filterHits(all, { ...none, text: 'ABOD' }))).toEqual(['Cee']);
    expect(names(filterHits(all, { ...none, text: 'beta' }))).toEqual(['Hub Anime', 'Cee']);
  });

  it('narrows by view and by language', () => {
    expect(names(filterHits(all, { ...none, view: 'available' }))).toEqual(['Movie Hub', 'Cee']);
    expect(names(filterHits(all, { ...none, view: 'installed' }))).toEqual(['Anime Land', 'Hub Anime']);
    expect(names(filterHits(all, { ...none, view: 'updates' }))).toEqual(['Hub Anime']);
    expect(names(filterHits(all, { ...none, language: 'EN' }))).toEqual(['Movie Hub', 'Hub Anime']);
  });
});

describe('languageChoices', () => {
  it('lists the languages most common first with how many extensions each has', () => {
    expect(languageChoices(all)).toEqual([
      { code: 'en', count: 2 },
      { code: 'ar', count: 1 },
      { code: 'ja', count: 1 },
    ]);
  });
});
