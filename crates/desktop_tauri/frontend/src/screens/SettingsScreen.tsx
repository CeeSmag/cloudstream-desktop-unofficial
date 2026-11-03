import React from 'react';
import { ProviderInfo } from '../types';
import { GeneralSettings } from './settings/GeneralSettings';
import { ExtensionsPage } from './settings/ExtensionsPage';

interface SettingsScreenProps {
  providers: ProviderInfo[];
  /** Shows a failure to the user. */
  onError: (error: unknown) => void;
}

type SettingsPage = 'general' | 'extensions';

const PAGES: { id: SettingsPage; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'extensions', label: 'Extensions' },
];

/** Settings, split into pages; extensions have a page of their own. */
export const SettingsScreen: React.FC<SettingsScreenProps> = ({ providers, onError }) => {
  const [page, setPage] = React.useState<SettingsPage>('extensions');

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

      {page === 'general' ? <GeneralSettings providers={providers} onError={onError} /> : <ExtensionsPage onError={onError} />}
    </div>
  );
};
