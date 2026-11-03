import React from 'react';
import { ArrowLeft, Search } from 'lucide-react';
import { IconTile } from '../../components/extensions/IconTile';
import type { ExtensionsState } from '../../hooks/useExtensions';
import { PluginEntry, PluginList } from '../../components/extensions/PluginList';
import { filterPlugins, PluginView, summarizePlugins } from '../../utils/extensionText';

interface RepositoryPageProps {
  extensions: ExtensionsState;
}

const VIEWS: { id: PluginView; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'available', label: 'Not installed' },
  { id: 'installed', label: 'Installed' },
  { id: 'updates', label: 'Updates' },
];

/** A full page for one repository: everything it offers, searchable, with install buttons. */
export const RepositoryPage: React.FC<RepositoryPageProps> = ({ extensions }) => {
  const [text, setText] = React.useState('');
  const [view, setView] = React.useState<PluginView>('all');
  const repository = extensions.repositories.find((entry) => entry.url === extensions.browsing);
  const shown = React.useMemo(() => filterPlugins(extensions.plugins, text, view), [extensions.plugins, text, view]);
  const filtering = text.trim() !== '' || view !== 'all';
  const entries: PluginEntry[] = React.useMemo(
    () => shown.map((plugin) => ({ key: plugin.internal_name, plugin })),
    [shown],
  );

  return (
    <div className="flex-1 overflow-y-auto page-gutter py-6 sm:py-8 w-full max-w-7xl mx-auto select-none space-y-6">
      <button
        onClick={extensions.closeRepository}
        className="flex items-center gap-2 px-3 py-1.5 rounded-xl bg-white/10 hover:bg-white/15 text-white text-xs font-medium transition-colors"
      >
        <ArrowLeft className="w-4 h-4" />
        <span>Back to extensions</span>
      </button>

      <div className="space-y-1">
        <h1 className="text-2xl sm:text-3xl font-extrabold text-white tracking-tight flex items-center gap-3 break-words">
          <IconTile url={repository?.iconUrl ?? null} name={repository?.name ?? 'Repository'} size="lg" />
          <span>{repository?.name ?? 'Repository'}</span>
        </h1>
        <p className="text-xs text-zinc-500 font-mono break-all">{extensions.browsing}</p>
        {!extensions.pluginsLoading && (
          <p className="text-xs text-zinc-400">{summarizePlugins(extensions.plugins)}</p>
        )}
      </div>

      <div className="flex flex-col sm:flex-row sm:items-center gap-3">
        <div className="relative flex-1 min-w-0">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500" />
          <input
            type="text"
            value={text}
            onChange={(event) => setText(event.target.value)}
            placeholder="Search extensions"
            className="w-full pl-9 pr-3 py-2.5 rounded-xl bg-surface-elevated border border-white/10 focus:border-accent text-white placeholder-zinc-500 text-xs focus:outline-none"
          />
        </div>
        <div className="flex flex-wrap items-center gap-1.5 p-1 rounded-xl bg-surface-elevated border border-white/5">
          {VIEWS.map((entry) => (
            <button
              key={entry.id}
              onClick={() => setView(entry.id)}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                view === entry.id ? 'bg-accent text-white font-semibold shadow-sm' : 'text-zinc-400 hover:text-white'
              }`}
            >
              {entry.label}
            </button>
          ))}
        </div>
      </div>

      <PluginList
        entries={entries}
        loading={extensions.pluginsLoading}
        busy={extensions.busy}
        emptyText={
          filtering ? 'No extension matches that search or filter.' : 'This repository does not offer any extensions.'
        }
        onInstall={(entry) => {
          if (extensions.browsing) void extensions.install(extensions.browsing, entry.plugin.internal_name);
        }}
      />
    </div>
  );
};
