import React from 'react';
import { ProviderInfo } from '../types';
import { GeneralSettings } from './settings/GeneralSettings';
import { ExtensionsPage, ExtensionsTab } from './settings/ExtensionsPage';
import { PreferencesPage } from './settings/PreferencesPage';
import { RepositoryPage } from './settings/RepositoryPage';
import { useExtensions } from '../hooks/useExtensions';

interface SettingsScreenProps {
  providers: ProviderInfo[];
  /** Jumps to a page each time `nonce` changes. */
  request?: { page: string; nonce: number };
  /** Shows a failure to the user. */
  onError: (error: unknown) => void;
}

type SettingsPage = 'general' | 'preferences' | 'extensions';

const PAGES: { id: SettingsPage; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'preferences', label: 'Preferences' },
  { id: 'extensions', label: 'Extensions' },
];

/** Settings, split into pages; extensions have a page of their own. */
export const SettingsScreen: React.FC<SettingsScreenProps> = ({ providers, request, onError }) => {
  const [page, setPage] = React.useState<SettingsPage>('extensions');
  const extensions = useExtensions(onError);
  React.useEffect(() => {
    if (request && request.nonce > 0 && PAGES.some((p) => p.id === request.page)) {
      setPage(request.page as SettingsPage);
      extensions.closeRepository?.();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request?.nonce]);
  // Kept here so coming back from a repository's page lands on the tab it was opened from.
  const [extensionsTab, setExtensionsTab] = React.useState<ExtensionsTab>('repositories');

  // An open repository takes the whole screen.
  if (extensions.browsing) return <RepositoryPage extensions={extensions} />;

  return (
    <div className="flex-1 overflow-y-auto page-gutter py-6 sm:py-8 max-w-4xl 2xl:max-w-6xl mx-auto w-full select-none space-y-6 sm:space-y-8">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl sm:text-3xl font-extrabold text-white tracking-tight">Settings</h1>
          <p className="text-xs text-zinc-400 mt-1">Extensions, repositories and how the app runs</p>
        </div>

        <div className="flex items-center gap-1.5 p-1 rounded-xl bg-surface-elevated border border-white/5">
          {PAGES.map((entry) => (
            <button
              key={entry.id}
              onClick={() => setPage(entry.id)}
              className={`px-4 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                page === entry.id ? 'bg-accent text-white font-semibold shadow-sm' : 'text-zinc-400 hover:text-white'
              }`}
            >
              {entry.label}
            </button>
          ))}
        </div>
      </div>

      {page === 'general' && <GeneralSettings providers={providers} onError={onError} />}
      {page === 'preferences' && <PreferencesPage onError={onError} />}
      {page === 'extensions' && <ExtensionsPage extensions={extensions} tab={extensionsTab} onTabChange={setExtensionsTab} onError={onError} />}
    </div>
  );
};
