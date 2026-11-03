import { RepositoryEntry, RepositoryPlugin } from '../types';

/** `6929` becomes `6.8 KB`; unknown sizes give an empty string. */
export function formatFileSize(bytes: number | null): string {
  if (bytes === null || bytes < 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Repositories whose name or address contains the text (case-insensitive); empty text keeps all. */
export function filterRepositories(repositories: RepositoryEntry[], text: string): RepositoryEntry[] {
  const needle = text.trim().toLowerCase();
  if (!needle) return repositories;
  return repositories.filter(
    (repository) => repository.name.toLowerCase().includes(needle) || repository.url.toLowerCase().includes(needle),
  );
}

/** The host of a repository address, such as `raw.githubusercontent.com`; the whole text if it is no address. */
export function repositoryHost(url: string): string {
  try {
    return new URL(url).host || url;
  } catch {
    return url;
  }
}

export type PluginAction = 'install' | 'update' | 'installed';

/** What the button of a plugin should offer: install it, update it, or show that it is current. */
export function pluginAction(plugin: RepositoryPlugin): PluginAction {
  if (plugin.installed_version === null) return 'install';
  return plugin.version > plugin.installed_version ? 'update' : 'installed';
}

/** Which plugins of a repository the page lists. */
export type PluginView = 'all' | 'available' | 'installed' | 'updates';

/** The plugins matching the search text (name, description, authors, language) and the chosen view. */
export function filterPlugins(plugins: RepositoryPlugin[], text: string, view: PluginView): RepositoryPlugin[] {
  const needle = text.trim().toLowerCase();
  return plugins.filter((plugin) => {
    const action = pluginAction(plugin);
    if (view === 'available' && action !== 'install') return false;
    if (view === 'installed' && action === 'install') return false;
    if (view === 'updates' && action !== 'update') return false;
    if (!needle) return true;
    const haystack = [plugin.name, plugin.description ?? '', plugin.authors.join(' '), plugin.language ?? '']
      .join(' ')
      .toLowerCase();
    return haystack.includes(needle);
  });
}

/** `12 extensions · 3 installed · 1 update` for the top of a repository page. */
export function summarizePlugins(plugins: RepositoryPlugin[]): string {
  const installed = plugins.filter((plugin) => pluginAction(plugin) !== 'install').length;
  const updates = plugins.filter((plugin) => pluginAction(plugin) === 'update').length;
  const parts = [`${plugins.length} extension${plugins.length === 1 ? '' : 's'}`, `${installed} installed`];
  if (updates > 0) parts.push(`${updates} update${updates === 1 ? '' : 's'}`);
  return parts.join(' · ');
}
