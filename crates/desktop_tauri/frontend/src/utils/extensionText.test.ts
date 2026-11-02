import { describe, expect, it } from 'vitest';
import { RepositoryEntry, RepositoryPlugin } from '../types';
import { filterRepositories, formatFileSize, pluginAction } from './extensionText';

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
