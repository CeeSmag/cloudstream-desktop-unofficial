import React from 'react';
import { Plus, Trash2, Globe, Cpu, CheckCircle } from 'lucide-react';
import { ProviderInfo, RepositoryEntry } from '../types';

interface SettingsScreenProps {
  repositories: RepositoryEntry[];
  providers: ProviderInfo[];
  onAddRepo: (url: string) => Promise<void>;
  onRemoveRepo: (url: string) => Promise<void>;
}

export const SettingsScreen: React.FC<SettingsScreenProps> = ({
  repositories,
  providers,
  onAddRepo,
  onRemoveRepo,
}) => {
  const [repoUrl, setRepoUrl] = React.useState('');
  const [loading, setLoading] = React.useState(false);
  const [msg, setMsg] = React.useState<string | null>(null);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!repoUrl.trim()) return;
    setLoading(true);
    setMsg(null);
    try {
      await onAddRepo(repoUrl.trim());
      setRepoUrl('');
      setMsg('Repository added successfully!');
    } catch (err) {
      setMsg(`Failed: ${String(err)}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex-1 overflow-y-auto page-gutter py-6 sm:py-8 max-w-4xl 2xl:max-w-6xl mx-auto w-full select-none space-y-8 sm:space-y-10">
      <div>
        <h1 className="text-2xl sm:text-2xl sm:text-3xl font-extrabold text-white tracking-tight">Settings</h1>
        <p className="text-xs text-zinc-400 mt-1">Configure extensions, player, and repositories</p>
      </div>

      {/* Extension Repositories */}
      <div className="p-4 sm:p-4 sm:p-6 rounded-2xl glass-panel space-y-5">
        <div>
          <h2 className="text-lg font-bold text-white flex items-center gap-2">
            <Globe className="w-5 h-5 text-accent" />
            <span>Extension Repositories</span>
          </h2>
          <p className="text-xs text-zinc-400 mt-1">
            Subscribe to remote repositories to install providers and extractors.
          </p>
        </div>

        {/* Add Input */}
        <form onSubmit={handleAdd} className="flex flex-col sm:flex-row gap-2">
          <input
            type="url"
            value={repoUrl}
            onChange={(e) => setRepoUrl(e.target.value)}
            placeholder="https://example.com/repo.json"
            className="flex-1 min-w-0 px-4 py-2.5 rounded-xl bg-surface-elevated border border-white/10 focus:border-accent text-white placeholder-zinc-500 text-xs focus:outline-none"
          />
          <button
            type="submit"
            disabled={loading}
            className="flex items-center justify-center space-x-1.5 px-4 py-2.5 rounded-xl bg-accent hover:bg-accent-hover text-white text-xs font-semibold shadow-md transition-colors disabled:opacity-50"
          >
            <Plus className="w-4 h-4" />
            <span>Add Repo</span>
          </button>
        </form>

        {msg && (
          <p className="text-xs text-accent font-medium">{msg}</p>
        )}

        {/* List of Repos */}
        <div className="space-y-2 mt-4">
          {repositories.map((repo) => (
            <div
              key={repo.url}
              className="flex items-center justify-between p-3.5 rounded-xl bg-surface-elevated/70 border border-white/5"
            >
              <div className="min-w-0 pr-4 flex-1">
                <h4 className="text-sm font-semibold text-white">{repo.name}</h4>
                <p className="text-xs text-zinc-500 truncate font-mono mt-0.5">{repo.url}</p>
              </div>
              <button
                onClick={() => onRemoveRepo(repo.url)}
                className="p-2 rounded-lg text-zinc-400 hover:text-rose-400 hover:bg-white/5 transition-colors"
                title="Remove Repository"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Installed Providers */}
      <div className="p-4 sm:p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
        <h2 className="text-lg font-bold text-white">Active Providers ({providers.length})</h2>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,16rem),1fr))] gap-3">
          {providers.map((p) => (
            <div
              key={p.name}
              className="flex items-center justify-between p-3 rounded-xl bg-surface-elevated/60 border border-white/5"
            >
              <div className="flex items-center space-x-2.5">
                <span className="w-2 h-2 rounded-full bg-emerald-400" />
                <span className="text-sm font-medium text-white">{p.name}</span>
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-zinc-400 uppercase">
                  {p.lang}
                </span>
              </div>
              <CheckCircle className="w-4 h-4 text-emerald-400" />
            </div>
          ))}
        </div>
      </div>

      {/* Engine & Diagnostics */}
      <div className="p-4 sm:p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
        <h2 className="text-lg font-bold text-white flex items-center gap-2">
          <Cpu className="w-5 h-5 text-accent" />
          <span>Engine Architecture</span>
        </h2>
        <div className="space-y-2 text-xs text-zinc-400">
          <div className="flex justify-between gap-4 py-1 border-b border-white/5">
            <span>Video Renderer</span>
            <span className="text-white font-medium text-right">libmpv (software frames drawn on a canvas)</span>
          </div>
          <div className="flex justify-between gap-4 py-1 border-b border-white/5">
            <span>UI Engine</span>
            <span className="text-white font-medium text-right">Tauri v2 + React</span>
          </div>
          <div className="flex justify-between gap-4 py-1 border-b border-white/5">
            <span>Extension Runtime</span>
            <span className="text-white font-medium text-right">JVM sidecar over JSON-lines RPC</span>
          </div>
        </div>
      </div>
    </div>
  );
};
