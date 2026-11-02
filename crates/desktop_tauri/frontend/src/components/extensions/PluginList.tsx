import React from 'react';
import { Download, Check, RefreshCw, Loader2 } from 'lucide-react';
import { RepositoryPlugin } from '../../types';
import { formatFileSize, pluginAction } from '../../utils/extensionText';

interface PluginListProps {
  plugins: RepositoryPlugin[];
  loading: boolean;
  busy: string | null;
  onInstall: (internalName: string) => void;
}

/** The extensions one repository offers, each with an install, update or installed button. */
export const PluginList: React.FC<PluginListProps> = ({ plugins, loading, busy, onInstall }) => {
  if (loading) {
    return (
      <div className="flex items-center gap-3 py-6 text-sm text-zinc-500">
        <Loader2 className="w-4 h-4 animate-spin text-accent" />
        <span>Loading extensions…</span>
      </div>
    );
  }
  if (plugins.length === 0) {
    return <p className="text-xs text-zinc-500">This repository does not offer any extensions.</p>;
  }

  return (
    <div className="space-y-2">
      {plugins.map((plugin) => {
        const action = pluginAction(plugin);
        const installing = busy === `install:${plugin.internal_name}`;
        const size = formatFileSize(plugin.file_size);
        return (
          <div key={plugin.internal_name} className="flex items-start gap-3 p-3.5 rounded-xl bg-surface-elevated/70 border border-white/5">
            <div className="flex-1 min-w-0">
              <h4 className="text-sm font-semibold text-white break-words">
                {plugin.name} <span className="text-xs font-normal text-zinc-500">v{plugin.version}</span>
                {plugin.language && (
                  <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-zinc-400 uppercase">
                    {plugin.language}
                  </span>
                )}
              </h4>
              {plugin.description && <p className="text-xs text-zinc-400 mt-1 break-words">{plugin.description}</p>}
              <p className="text-[11px] text-zinc-600 mt-1">
                {[plugin.authors.join(', '), size].filter(Boolean).join(' · ')}
              </p>
            </div>

            <button
              onClick={() => onInstall(plugin.internal_name)}
              disabled={action === 'installed' || busy !== null}
              className={`flex flex-none items-center gap-1.5 px-3.5 py-2 rounded-lg text-xs font-semibold transition-colors disabled:opacity-60 ${
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
