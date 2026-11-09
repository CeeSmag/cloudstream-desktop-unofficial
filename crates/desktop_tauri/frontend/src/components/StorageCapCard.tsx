import React from 'react';
import { HardDrive } from 'lucide-react';
import { api } from '../services/api';
import { StorageInfo } from '../types';
import { formatFileSize } from '../utils/fileSize';
import { describeValue, logEvent } from '../utils/logger';
import { capLabel, STORAGE_CAP_OPTIONS, usedPercent } from '../utils/storageCap';

interface StorageCapCardProps {
  onError: (error: unknown) => void;
}

/** The limit for the app's data folder (1 to 10 GB): what is over it and can be made again is deleted. */
export const StorageCapCard: React.FC<StorageCapCardProps> = ({ onError }) => {
  const [info, setInfo] = React.useState<StorageInfo | null>(null);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    let live = true;
    api
      .getStorageCap()
      .then((value) => {
        if (live) setInfo(value);
      })
      .catch((error: unknown) => {
        logEvent('error', `cannot read the data folder limit: ${describeValue(error)}`);
        onError(error);
      });
    return () => {
      live = false;
    };
  }, [onError]);

  const choose = async (gb: number) => {
    setBusy(true);
    try {
      logEvent('info', `the viewer chose a ${gb} GB data folder limit`);
      setInfo(await api.setStorageCap(gb));
    } catch (error) {
      logEvent('error', `cannot set the data folder limit to ${gb} GB: ${describeValue(error)}`);
      onError(error);
    } finally {
      setBusy(false);
    }
  };

  const percent = info ? usedPercent(info.used_bytes, info.cap_gb) : 0;
  return (
    <div className="p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 space-y-1">
          <h2 className="text-lg font-bold text-white flex items-center gap-2">
            <HardDrive className="w-5 h-5 text-accent flex-none" />
            <span>Storage limit</span>
          </h2>
          <p className="text-xs text-zinc-400">
            The most disk space the app's data folder (Local\CloudStreamRs) may use. Over the limit, things that can be
            downloaded or made again are deleted, oldest first: older program versions, finished debug logs, subtitles, saved
            stream sources and the check browser's cache. Your library, watch progress, plugins and settings are never deleted.
          </p>
        </div>
        <select
          aria-label="Storage limit"
          value={info?.cap_gb ?? ''}
          disabled={busy || info === null}
          onChange={(event) => void choose(Number(event.target.value))}
          className="flex-none rounded-lg bg-white/10 px-3 py-2 text-sm text-white outline-none disabled:opacity-60"
        >
          {info === null && <option value="">...</option>}
          {STORAGE_CAP_OPTIONS.map((gb) => (
            <option key={gb} value={gb} className="bg-zinc-900">
              {capLabel(gb)}
            </option>
          ))}
        </select>
      </div>

      {info && (
        <div className="space-y-2">
          <div className="h-2 w-full overflow-hidden rounded-full bg-white/10">
            <div className={`h-full ${info.over ? 'bg-red-400' : 'bg-accent'}`} style={{ width: `${percent}%` }} />
          </div>
          <p className="text-xs text-zinc-400">
            Using {formatFileSize(info.used_bytes)} of {capLabel(info.cap_gb)}.
            {info.over && (
              <span className="text-amber-300">
                {' '}
                Over the limit: what is left cannot be deleted without losing your data, so choose a larger limit.
              </span>
            )}
          </p>
        </div>
      )}
    </div>
  );
};
