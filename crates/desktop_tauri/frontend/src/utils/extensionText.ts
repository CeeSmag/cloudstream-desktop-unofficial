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

export type PluginAction = 'install' | 'update' | 'installed';

/** What the button of a plugin should offer: install it, update it, or show that it is current. */
export function pluginAction(plugin: RepositoryPlugin): PluginAction {
  if (plugin.installed_version === null) return 'install';
  return plugin.version > plugin.installed_version ? 'update' : 'installed';
}
