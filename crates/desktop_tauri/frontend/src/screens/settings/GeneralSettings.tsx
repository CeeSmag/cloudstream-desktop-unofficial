import React from 'react';
import { Cpu, CheckCircle } from 'lucide-react';
import { ProviderInfo } from '../../types';
import { DebugLoggingCard } from '../../components/DebugLoggingCard';
import { UpdateCard } from '../../components/UpdateCard';
import { DeleteDataCard } from '../../components/DeleteDataCard';
import { StorageCapCard } from '../../components/StorageCapCard';

interface GeneralSettingsProps {
  providers: ProviderInfo[];
  onError: (error: unknown) => void;
}

const SECTION = 'p-4 sm:p-6 rounded-2xl glass-panel space-y-4';

/** Providers that are active now, and how the app is put together. */
export const GeneralSettings: React.FC<GeneralSettingsProps> = ({ providers, onError }) => (
  <div className="space-y-8 sm:space-y-10">
    <UpdateCard onError={onError} />
    <DebugLoggingCard onError={onError} />
    <StorageCapCard onError={onError} />

    <div className={SECTION}>
      <h2 className="text-lg font-bold text-white">Active Providers ({providers.length})</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,16rem),1fr))] gap-3">
        {providers.map((provider) => (
          <div
            key={provider.name}
            className="flex items-center justify-between p-3 rounded-xl bg-surface-elevated/60 border border-white/5"
          >
            <div className="flex items-center space-x-2.5 min-w-0">
              <span className="w-2 h-2 flex-none rounded-full bg-emerald-400" />
              <span className="text-sm font-medium text-white truncate">{provider.name}</span>
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-zinc-400 uppercase">{provider.lang}</span>
            </div>
            <CheckCircle className="w-4 h-4 flex-none text-emerald-400" />
          </div>
        ))}
        {providers.length === 0 && <p className="text-xs text-zinc-500">No providers yet. Install an extension first.</p>}
      </div>
    </div>

    <div className={SECTION}>
      <h2 className="text-lg font-bold text-white flex items-center gap-2">
        <Cpu className="w-5 h-5 text-accent" />
        <span>Engine Architecture</span>
      </h2>
      <div className="space-y-2 text-xs text-zinc-400">
        {[
          ['Video Renderer', 'libmpv (software frames drawn on a canvas)'],
          ['UI Engine', 'Tauri v2 + React'],
          ['Extension Runtime', 'JVM sidecar over JSON-lines RPC'],
        ].map(([label, value]) => (
          <div key={label} className="flex justify-between gap-4 py-1 border-b border-white/5">
            <span>{label}</span>
            <span className="text-white font-medium text-right">{value}</span>
          </div>
        ))}
      </div>
    </div>

    <DeleteDataCard onError={onError} />
  </div>
);
