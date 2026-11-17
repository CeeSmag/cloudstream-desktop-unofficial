import { RepositoryEntry } from '../types';

/** How the Repositories list can be ordered. */
export type RepositorySort = 'name' | 'date';

/** Copies and sorts repositories: name A–Z, or date added newest first. */
export function sortRepositories(repositories: RepositoryEntry[], sort: RepositorySort): RepositoryEntry[] {
  const copy = [...repositories];
  if (sort === 'name') {
    copy.sort((a, b) => a.name.localeCompare(b.name) || a.url.localeCompare(b.url));
    return copy;
  }
  copy.sort((a, b) => b.addedAtMs - a.addedAtMs || a.name.localeCompare(b.name));
  return copy;
}
