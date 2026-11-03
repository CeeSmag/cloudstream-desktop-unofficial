import { describe, expect, it } from 'vitest';
import { RepositoryEntry, RepositoryPlugin } from '../types';
import {
  filterPlugins,
  filterRepositories,
  formatFileSize,
  pluginAction,
  repositoryHost,
  summarizePlugins,
} from './extensionText';

const repository = (name: string, url: string): RepositoryEntry => ({ name, url, addedAtMs: 0 });

const plugin = (version: number, installed: number | null): RepositoryPlugin => ({
  internal_name: 'P',
  name: 'P',
  version,
  description: null,
  authors: [],
  language: null,
  icon_url: null,
  file_size: null,
  installed_version: installed,
});

describe('formatFileSize', () => {
  it('uses bytes, kilobytes and megabytes', () => {
    expect(formatFileSize(512)).toBe('512 B');
    expect(formatFileSize(6929)).toBe('6.8 KB');
    expect(formatFileSize(5 * 1024 * 1024)).toBe('5.0 MB');
  });

  it('is empty when the size is unknown', () => {
    expect(formatFileSize(null)).toBe('');
  });
});

describe('filterRepositories', () => {
  const all = [repository('Alpha Repo', 'https://a.test/repo.json'), repository('Beta', 'https://b.test/x.json')];

  it('keeps everything for empty text', () => {
    expect(filterRepositories(all, '  ')).toHaveLength(2);
  });

  it('matches name or address regardless of case', () => {
    expect(filterRepositories(all, 'ALPHA')).toHaveLength(1);
    expect(filterRepositories(all, 'b.test')).toHaveLength(1);
    expect(filterRepositories(all, 'zzz')).toHaveLength(0);
  });
});

describe('pluginAction', () => {
  it('offers install, update, or shows installed', () => {
    expect(pluginAction(plugin(1, null))).toBe('install');
    expect(pluginAction(plugin(3, 2))).toBe('update');
    expect(pluginAction(plugin(2, 2))).toBe('installed');
  });
});

describe('repositoryHost', () => {
  it('shows only the host of an address', () => {
    expect(repositoryHost('https://raw.githubusercontent.com/a/b/repo.json')).toBe('raw.githubusercontent.com');
  });

  it('keeps text that is not an address as it is', () => {
    expect(repositoryHost('not an address')).toBe('not an address');
  });
});

describe('filterPlugins and summarizePlugins', () => {
  const named = (name: string, version: number, installed: number | null, extra: Partial<RepositoryPlugin> = {}) => ({
    ...plugin(version, installed),
    internal_name: name,
    name,
    ...extra,
  });
  const all = [
    named('Alpha', 1, null, { description: 'Movies and series', authors: ['Dana'], language: 'en' }),
    named('Beta', 2, 2),
    named('Gamma', 3, 2),
  ];

  it('narrows by view', () => {
    expect(filterPlugins(all, '', 'available').map((p) => p.name)).toEqual(['Alpha']);
    expect(filterPlugins(all, '', 'installed').map((p) => p.name)).toEqual(['Beta', 'Gamma']);
    expect(filterPlugins(all, '', 'updates').map((p) => p.name)).toEqual(['Gamma']);
    expect(filterPlugins(all, '', 'all')).toHaveLength(3);
  });

  it('searches name, description, authors and language', () => {
    expect(filterPlugins(all, 'series', 'all').map((p) => p.name)).toEqual(['Alpha']);
    expect(filterPlugins(all, 'dana', 'all')).toHaveLength(1);
    expect(filterPlugins(all, 'GAMMA', 'all')).toHaveLength(1);
    expect(filterPlugins(all, 'zzz', 'all')).toHaveLength(0);
  });

  it('summarises how many extensions are installed and have updates', () => {
    expect(summarizePlugins(all)).toBe('3 extensions · 2 installed · 1 update');
    expect(summarizePlugins([all[0]])).toBe('1 extension · 0 installed');
  });
});
