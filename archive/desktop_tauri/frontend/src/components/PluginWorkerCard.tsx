import React from 'react';
import { Layers } from 'lucide-react';
import { api } from '../services/api';
import { PluginWorkerLimit } from '../types';
import { describeValue, logEvent } from '../utils/logger';

interface PluginWorkerCardProps {
  onError: (error: unknown) => void;
}

const SELECT = 'flex-none rounded-lg bg-white/10 px-3 py-2 text-sm text-white outline-none disabled:opacity-60';

function labelFor(limit: number): string {
  if (limit === 0) return 'Unlimited';
  if (limit === 1) return '1 (low-end)';
  return String(limit);
}

/** How many plugin JVMs may stay running at once. */
export const PluginWorkerCard: React.FC<PluginWorkerCardProps> = ({ onError }) => {
  const [setting, setSetting] = React.useState<PluginWorkerLimit | null>(null);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    let live = true;
    api
      .getPluginWorkerLimit()
      .then((value) => {
        if (live) setSetting(value);
      })
      .catch((error: unknown) => {
        logEvent('error', `cannot read plugin process limit: ${describeValue(error)}`);
        onError(error);
      });
    return () => {
      live = false;
    };
  }, [onError]);

  const choose = async (limit: number) => {
    setBusy(true);
    try {
      logEvent('info', `the viewer set plugin processes to ${labelFor(limit)}`);
      setSetting(await api.setPluginWorkerLimit(limit));
    } catch (error) {
      logEvent('error', `cannot set plugin process limit: ${describeValue(error)}`);
      onError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
      <div className="min-w-0 space-y-1">
        <h2 className="text-lg font-bold text-white flex items-center gap-2">
          <Layers className="w-5 h-5 text-accent flex-none" />
          <span>Plugin processes (low-end)</span>
        </h2>
        <p className="text-xs text-zinc-400">
          How many plugin Java processes stay in memory. Keeping one frees RAM on slower PCs; switching
          provider may take a few seconds the first time while that plugin starts again. Unlimited keeps
          every installed plugin ready.
        </p>
      </div>
      <label className="flex items-center gap-2 text-sm text-zinc-300">
        Limit
        <select
          aria-label="Plugin process limit"
          value={setting?.limit ?? ''}
          disabled={busy || setting === null}
          onChange={(event) => void choose(Number(event.target.value))}
          className={SELECT}
        >
          {setting === null && <option value="">...</option>}
          {setting?.choices.map((choice) => (
            <option key={choice} value={choice} className="bg-zinc-900">
              {labelFor(choice)}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
};
