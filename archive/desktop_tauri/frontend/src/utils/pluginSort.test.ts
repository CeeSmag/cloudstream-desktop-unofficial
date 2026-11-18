import { describe, expect, it } from 'vitest';
import { PluginHit, RepositoryPlugin } from '../types';
import { sortHits, sortPlugins } from './pluginSort';

const plugin = (name: string, over: Partial<RepositoryPlugin> = {}): RepositoryPlugin => ({
  internal_name: name.replace(/\s/g, ''),
  name,
  version: 1,
  description: null,
  authors: [],
  language: null,
  icon_url: null,
  file_size: null,
  installed_version: null,
  installed_at_ms: null,
  ...over,
});

const hit = (name: string, over: Partial<RepositoryPlugin> = {}): PluginHit => ({
  repository_url: 'https://r/repo.json',
  repository_name: 'R',
  repository_icon_url: null,
  plugin: plugin(name, over),
});

const names = (plugins: RepositoryPlugin[]) => plugins.map((p) => p.name);

describe('sortPlugins', () => {
  it('orders by name A to Z', () => {
    expect(names(sortPlugins([plugin('Charlie'), plugin('Alpha'), plugin('Bravo')], 'name'))).toEqual([
      'Alpha',
      'Bravo',
      'Charlie',
    ]);
  });

  it('orders by size largest first and puts unknown sizes last', () => {
    expect(
      names(
        sortPlugins(
          [plugin('Small', { file_size: 10 }), plugin('Unknown'), plugin('Big', { file_size: 100 })],
          'size',
        ),
      ),
    ).toEqual(['Big', 'Small', 'Unknown']);
  });

  it('orders by install date newest first; not installed and zero last', () => {
    expect(
      names(
        sortPlugins(
          [
            plugin('Old', { installed_at_ms: 100, installed_version: 1 }),
            plugin('New', { installed_at_ms: 300, installed_version: 1 }),
            plugin('Legacy', { installed_at_ms: 0, installed_version: 1 }),
            plugin('Missing'),
          ],
          'date',
        ),
      ),
    ).toEqual(['New', 'Old', 'Legacy', 'Missing']);
  });

  it('orders by language with preferred codes first, then A to Z, missing last', () => {
    expect(
      names(
        sortPlugins(
          [
            plugin('Japanese', { language: 'ja' }),
            plugin('Arabic', { language: 'ar' }),
            plugin('English', { language: 'en' }),
            plugin('None'),
            plugin('French', { language: 'fr' }),
          ],
          'language',
          ['ar', 'en'],
        ),
      ),
    ).toEqual(['Arabic', 'English', 'French', 'Japanese', 'None']);
  });
});

describe('sortHits', () => {
  it('orders catalog hits the same way as plugins', () => {
    const hits = [hit('B', { language: 'en' }), hit('A', { language: 'ar' })];
    expect(sortHits(hits, 'language', ['en']).map((h) => h.plugin.name)).toEqual(['B', 'A']);
  });
});
