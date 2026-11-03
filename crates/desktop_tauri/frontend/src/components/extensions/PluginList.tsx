import React from 'react';
import { Download, Check, RefreshCw, Loader2 } from 'lucide-react';
import { RepositoryPlugin } from '../../types';
import { formatFileSize, pluginAction } from '../../utils/extensionText';
import { IconTile } from './IconTile';

/** One card of the list: an extension and, when the list mixes repositories, which one offers it. */
export interface PluginEntry {
  /** Unique among the entries (the same extension may be offered by several repositories). */
  key: string;
  plugin: RepositoryPlugin;
  repository?: { name: string; url: string };
}

interface PluginListProps {
  entries: PluginEntry[];
  loading: boolean;
  busy: string | null;
  onInstall: (entry: PluginEntry) => void;
  /** Makes the repository name of a card a link; used where cards come from several repositories. */
  onOpenRepository?: (url: string) => void;
  /** Said when nothing is listed and the reason is a search or filter rather than an empty list. */
  emptyText: string;
}

/** Extensions as a grid of cards, each with its icon, the full description and an install button. */
export const PluginList: React.FC<PluginListProps> = ({ entries, loading, busy, onInstall, onOpenRepository, emptyText }) => {
  if (loading) {
    return (
      <div className="flex items-center gap-3 py-10 text-sm text-zinc-500">
        <Loader2 className="w-5 h-5 animate-spin text-accent" />
        <span>Loading extensions…</span>
      </div>
    );
  }
  if (entries.length === 0) {
    return <p className="py-6 text-sm text-zinc-500">{emptyText}</p>;
  }

  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,24rem),1fr))] gap-3">
      {entries.map((entry) => {
        const { plugin, repository } = entry;
        const action = pluginAction(plugin);
        const installing = busy === `install:${plugin.internal_name}`;
        const details = [plugin.authors.join(', '), formatFileSize(plugin.file_size)].filter(Boolean).join(' · ');
        return (
          <div key={entry.key} className="flex flex-col gap-3 p-4 rounded-2xl glass-panel">
            <div className="flex flex-1 min-w-0 items-start gap-3">
              <IconTile url={plugin.icon_url} name={plugin.name} size="md" />
              <div className="flex-1 min-w-0 space-y-1.5">
                <h4 className="text-sm font-semibold text-white break-words">
                  {plugin.name} <span className="text-xs font-normal text-zinc-500">v{plugin.version}</span>
                  {plugin.language && (
                    <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-zinc-400 uppercase">
                      {plugin.language}
                    </span>
                  )}
                </h4>
                {plugin.description && <p className="text-xs text-zinc-300 break-words">{plugin.description}</p>}
                <p className="text-[11px] text-zinc-500 break-words">
                  {repository && (
                    <>
                      from{' '}
                      {onOpenRepository ? (
                        <button onClick={() => onOpenRepository(repository.url)} className="text-accent hover:text-accent-hover font-medium">
                          {repository.name}
                        </button>
                      ) : (
                        repository.name
                      )}
                      {details && ' · '}
                    </>
                  )}
                  {details}
                </p>
              </div>
            </div>

            <button
              onClick={() => onInstall(entry)}
              disabled={action === 'installed' || busy !== null}
              className={`flex items-center justify-center gap-1.5 px-3.5 py-2 rounded-lg text-xs font-semibold transition-colors disabled:opacity-60 ${
                action === 'installed'
                  ? 'bg-emerald-500/15 text-emerald-300'
                  : 'bg-accent hover:bg-accent-hover text-white'
              }`}
            >
              {installing ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : action === 'installed' ? (
                <Check className="w-3.5 h-3.5" />
              ) : action === 'update' ? (
                <RefreshCw className="w-3.5 h-3.5" />
              ) : (
                <Download className="w-3.5 h-3.5" />
              )}
              <span>{installing ? 'Installing…' : action === 'installed' ? 'Installed' : action === 'update' ? 'Update' : 'Install'}</span>
            </button>
          </div>
        );
      })}
    </div>
  );
};
