import React from 'react';
import type { ExtensionsState } from '../../hooks/useExtensions';
import { InstalledExtensionList } from '../../components/extensions/InstalledExtensionList';
import { RepositoryPanel } from '../../components/extensions/RepositoryPanel';
import { PluginsTab } from './PluginsTab';

/** The three parts of the Extensions page. */
export type ExtensionsTab = 'repositories' | 'installed' | 'plugins';

interface ExtensionsPageProps {
  extensions: ExtensionsState;
  tab: ExtensionsTab;
  onTabChange: (tab: ExtensionsTab) => void;
  /** Shows a failure to the user. */
  onError: (error: unknown) => void;
}

/** Repositories, the installed extensions, and every extension of every repository, one tab each. */
export const ExtensionsPage: React.FC<ExtensionsPageProps> = ({ extensions, tab, onTabChange, onError }) => {
  const tabs: { id: ExtensionsTab; label: string }[] = [
    { id: 'repositories', label: `Repositories (${extensions.repositories.length})` },
    { id: 'installed', label: `Installed (${extensions.installed.length})` },
    { id: 'plugins', label: 'Plugins' },
  ];

  return (
    <div className="space-y-5">
      <div role="tablist" className="flex flex-wrap items-center gap-1.5 p-1 rounded-xl bg-surface-elevated border border-white/5 w-fit max-w-full">
        {tabs.map((entry) => (
          <button
            key={entry.id}
            role="tab"
            aria-selected={tab === entry.id}
            onClick={() => onTabChange(entry.id)}
            className={`px-4 py-1.5 rounded-lg text-xs font-medium transition-colors ${
              tab === entry.id ? 'bg-accent text-white font-semibold shadow-sm' : 'text-zinc-400 hover:text-white'
            }`}
          >
            {entry.label}
          </button>
        ))}
      </div>

      {tab === 'repositories' && (
        <div className="p-4 sm:p-5 rounded-2xl glass-panel space-y-3">
          <p className="text-[11px] text-zinc-500">Open a repository to see all the extensions it offers.</p>
          <RepositoryPanel
            repositories={extensions.repositories}
            busy={extensions.busy}
            onAdd={extensions.addRepository}
            onRemove={extensions.removeRepository}
            onOpen={extensions.browse}
          />
        </div>
      )}

      {tab === 'installed' && (
        <div className="p-4 sm:p-5 rounded-2xl glass-panel space-y-3">
          <p className="text-[11px] text-zinc-500">Extensions run in a sandbox. Trust one only if you know its author.</p>
          <InstalledExtensionList
            extensions={extensions.installed}
            busy={extensions.busy}
            onUninstall={extensions.uninstall}
            onSetTrusted={extensions.setTrusted}
          />
        </div>
      )}

      {tab === 'plugins' && <PluginsTab extensions={extensions} onError={onError} />}
    </div>
  );
};
