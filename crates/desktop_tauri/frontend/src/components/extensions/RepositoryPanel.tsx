import React from 'react';
import { Plus, Trash2, Search, ChevronRight } from 'lucide-react';
import { RepositoryEntry } from '../../types';
import { filterRepositories, repositoryHost } from '../../utils/extensionText';

interface RepositoryPanelProps {
  repositories: RepositoryEntry[];
  busy: string | null;
  onAdd: (url: string) => Promise<boolean>;
  onRemove: (url: string) => void;
  /** Opens the page of one repository. */
  onOpen: (url: string) => void;
}

/** Repositories shown without a search or "show all" until the list is longer than this. */
const SHOWN_AT_FIRST = 8;
/** The search box appears once there are more repositories than this. */
const SEARCH_FROM = 8;

/** Repositories the user subscribed to: add by address, search, remove, or open one to see its extensions. */
export const RepositoryPanel: React.FC<RepositoryPanelProps> = ({ repositories, busy, onAdd, onRemove, onOpen }) => {
  const [address, setAddress] = React.useState('');
  const [filter, setFilter] = React.useState('');
  const [showAll, setShowAll] = React.useState(false);
  const matching = React.useMemo(() => filterRepositories(repositories, filter), [repositories, filter]);
  const searching = filter.trim() !== '';
  const shown = showAll || searching ? matching : matching.slice(0, SHOWN_AT_FIRST);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const url = address.trim();
    if (url && (await onAdd(url))) setAddress('');
  };

  return (
    <div className="space-y-3">
      <form onSubmit={submit} className="flex flex-col sm:flex-row gap-2">
        <input
          type="url"
          value={address}
          onChange={(event) => setAddress(event.target.value)}
          placeholder="Add a repository: https://example.com/repo.json"
          className="flex-1 min-w-0 px-4 py-2 rounded-xl bg-surface-elevated border border-white/10 focus:border-accent text-white placeholder-zinc-500 text-xs focus:outline-none"
        />
        <button
          type="submit"
          disabled={busy === 'add-repository'}
          className="flex items-center justify-center gap-1.5 px-4 py-2 rounded-xl bg-accent hover:bg-accent-hover text-white text-xs font-semibold transition-colors disabled:opacity-50"
        >
          <Plus className="w-4 h-4" />
          <span>{busy === 'add-repository' ? 'Adding…' : 'Add'}</span>
        </button>
      </form>

      {repositories.length > SEARCH_FROM && (
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
          <input
            type="text"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder={`Search ${repositories.length} repositories`}
            className="w-full pl-9 pr-3 py-2 rounded-xl bg-surface-elevated/80 border border-white/10 focus:border-accent text-white placeholder-zinc-500 text-xs focus:outline-none"
          />
        </div>
      )}

      {repositories.length === 0 ? (
        <p className="text-xs text-zinc-500">No repositories yet. Add one above to see the extensions it offers.</p>
      ) : (
        <div className="divide-y divide-white/5 rounded-xl border border-white/5 overflow-hidden">
          {shown.map((repository) => (
            <div key={repository.url} className="flex items-center gap-1 bg-surface-elevated/50 hover:bg-surface-elevated transition-colors">
              <button onClick={() => onOpen(repository.url)} className="flex flex-1 min-w-0 items-center gap-3 px-3.5 py-2.5 text-left" title={repository.url}>
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium text-white break-words">{repository.name}</span>
                  <span className="block text-[11px] text-zinc-500 break-all">{repositoryHost(repository.url)}</span>
                </span>
                <ChevronRight className="w-4 h-4 flex-none text-zinc-600" />
              </button>
              <button
                onClick={() => onRemove(repository.url)}
                disabled={busy === `remove-repository:${repository.url}`}
                className="mr-1.5 p-2 rounded-lg text-zinc-500 hover:text-rose-400 hover:bg-white/5 transition-colors disabled:opacity-50"
                title="Remove repository"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </div>
          ))}
          {shown.length === 0 && <p className="px-3.5 py-3 text-xs text-zinc-500">No repository matches that search.</p>}
        </div>
      )}

      {!searching && matching.length > SHOWN_AT_FIRST && (
        <button onClick={() => setShowAll(!showAll)} className="text-xs font-medium text-accent hover:text-accent-hover">
          {showAll ? 'Show fewer' : `Show all ${matching.length}`}
        </button>
      )}
    </div>
  );
};
