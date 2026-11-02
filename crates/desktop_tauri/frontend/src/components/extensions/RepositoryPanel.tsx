import React from 'react';
import { Plus, Trash2, Search, ChevronRight } from 'lucide-react';
import { RepositoryEntry } from '../../types';
import { filterRepositories } from '../../utils/extensionText';

interface RepositoryPanelProps {
  repositories: RepositoryEntry[];
  browsing: string | null;
  busy: string | null;
  onAdd: (url: string) => Promise<boolean>;
  onRemove: (url: string) => void;
  onBrowse: (url: string) => void;
}

/** Repositories the user subscribed to: add by address, search, remove, or pick one to browse. */
export const RepositoryPanel: React.FC<RepositoryPanelProps> = ({
  repositories,
  browsing,
  busy,
  onAdd,
  onRemove,
  onBrowse,
}) => {
  const [address, setAddress] = React.useState('');
  const [filter, setFilter] = React.useState('');
  const shown = React.useMemo(() => filterRepositories(repositories, filter), [repositories, filter]);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const url = address.trim();
    if (url && (await onAdd(url))) setAddress('');
  };

  return (
    <div className="space-y-4">
      <form onSubmit={submit} className="flex flex-col sm:flex-row gap-2">
        <input
          type="url"
          value={address}
          onChange={(event) => setAddress(event.target.value)}
          placeholder="https://example.com/repo.json"
          className="flex-1 min-w-0 px-4 py-2.5 rounded-xl bg-surface-elevated border border-white/10 focus:border-accent text-white placeholder-zinc-500 text-xs focus:outline-none"
        />
        <button
          type="submit"
          disabled={busy === 'add-repository'}
          className="flex items-center justify-center gap-1.5 px-4 py-2.5 rounded-xl bg-accent hover:bg-accent-hover text-white text-xs font-semibold shadow-md transition-colors disabled:opacity-50"
        >
          <Plus className="w-4 h-4" />
          <span>{busy === 'add-repository' ? 'Adding…' : 'Add repository'}</span>
        </button>
      </form>

      {repositories.length > 6 && (
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
        <div className="space-y-1.5 max-h-96 overflow-y-auto pr-1">
          {shown.map((repository) => (
            <div
              key={repository.url}
              className={`flex items-center gap-2 p-2.5 rounded-xl border transition-colors ${
                browsing === repository.url
                  ? 'bg-accent/15 border-accent/30'
                  : 'bg-surface-elevated/60 border-white/5 hover:border-white/15'
              }`}
            >
              <button onClick={() => onBrowse(repository.url)} className="flex-1 min-w-0 text-left" title="Browse extensions">
                <h4 className="text-sm font-semibold text-white break-words">{repository.name}</h4>
                <p className="text-[11px] text-zinc-500 font-mono break-all">{repository.url}</p>
              </button>
              <ChevronRight className="w-4 h-4 flex-none text-zinc-600" />
              <button
                onClick={() => onRemove(repository.url)}
                disabled={busy === `remove-repository:${repository.url}`}
                className="p-2 rounded-lg text-zinc-400 hover:text-rose-400 hover:bg-white/5 transition-colors disabled:opacity-50"
                title="Remove repository"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </div>
          ))}
          {shown.length === 0 && <p className="text-xs text-zinc-500">No repository matches that search.</p>}
        </div>
      )}
    </div>
  );
};
