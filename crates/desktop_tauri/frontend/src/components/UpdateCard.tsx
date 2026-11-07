import React from 'react';
import { RefreshCw, Download, CheckCircle, Sparkles } from 'lucide-react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { api, describeError } from '../services/api';
import { UpdateCheckResult, UpdateProgressPayload } from '../types';

interface UpdateCardProps {
  onError: (error: unknown) => void;
}

const SECTION = 'p-4 sm:p-6 rounded-2xl glass-panel space-y-4';

export const UpdateCard: React.FC<UpdateCardProps> = ({ onError }) => {
  const [checking, setChecking] = React.useState(false);
  const [updating, setUpdating] = React.useState(false);
  const [progress, setProgress] = React.useState<UpdateProgressPayload | null>(null);
  const [updateInfo, setUpdateInfo] = React.useState<UpdateCheckResult | null>(null);
  const [statusMessage, setStatusMessage] = React.useState<string | null>(null);

  React.useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen<UpdateProgressPayload>('update-progress', (event) => {
      setProgress(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleCheckUpdate = async () => {
    setChecking(true);
    setStatusMessage(null);
    try {
      const result = await api.checkForUpdates();
      setUpdateInfo(result);
      if (!result.has_update) {
        setStatusMessage(`You are on the latest version (v${result.current_version}).`);
      }
    } catch (error) {
      onError(error);
      setStatusMessage(`Update check failed: ${describeError(error)}`);
    } finally {
      setChecking(false);
    }
  };

  const handleUpdateNow = async () => {
    if (!updateInfo?.download_url) return;
    setUpdating(true);
    try {
      await api.applyUpdate(updateInfo.download_url);
    } catch (error) {
      onError(error);
      setStatusMessage(`Update failed: ${describeError(error)}`);
      setUpdating(false);
    }
  };

  const formatBytes = (bytes: number) => {
    if (bytes <= 0) return '0 MB';
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className={SECTION}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-bold text-white flex items-center gap-2">
            <Sparkles className="w-5 h-5 text-accent" />
            <span>Software Updates</span>
          </h2>
          <p className="text-xs text-zinc-400 mt-0.5">
            Check GitHub releases for application and sidecar updates
          </p>
        </div>

        <button
          onClick={handleCheckUpdate}
          disabled={checking || updating}
          className="flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-xs font-semibold bg-white/10 text-white hover:bg-white/15 transition-all disabled:opacity-50"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${checking ? 'animate-spin' : ''}`} />
          <span>{checking ? 'Checking...' : 'Check for update'}</span>
        </button>
      </div>

      {statusMessage && !updateInfo?.has_update && (
        <div className="flex items-center gap-2 p-3 rounded-xl bg-surface-elevated/40 border border-white/5 text-xs text-zinc-300">
          <CheckCircle className="w-4 h-4 text-emerald-400 flex-none" />
          <span>{statusMessage}</span>
        </div>
      )}

      {updateInfo?.has_update && (
        <div className="p-4 rounded-xl bg-accent/10 border border-accent/20 space-y-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-accent animate-pulse" />
                <span className="text-sm font-bold text-white">
                  CloudStream v{updateInfo.latest_version} Available
                </span>
                {updateInfo.asset_size && (
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-zinc-300">
                    {formatBytes(updateInfo.asset_size)}
                  </span>
                )}
              </div>
              <p className="text-xs text-zinc-300 mt-1">
                A newer release is available on GitHub. You are running v{updateInfo.current_version}.
              </p>
            </div>

            {!updating && updateInfo.download_url && (
              <button
                onClick={handleUpdateNow}
                className="flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-bold bg-accent text-white hover:bg-accent/80 shadow-md transition-all flex-none"
              >
                <Download className="w-3.5 h-3.5" />
                <span>Update Now</span>
              </button>
            )}
          </div>

          {updating && (
            <div className="space-y-2 pt-1 border-t border-white/10">
              <div className="flex justify-between text-xs text-zinc-300">
                <span>{progress?.percent === 100 ? 'Restarting application...' : 'Downloading update payload...'}</span>
                <span>
                  {progress ? `${progress.percent}% (${formatBytes(progress.downloaded_bytes)} / ${formatBytes(progress.total_bytes)})` : 'Connecting...'}
                </span>
              </div>
              <div className="w-full h-2 rounded-full bg-white/10 overflow-hidden">
                <div
                  className="h-full bg-accent transition-all duration-300 ease-out"
                  style={{ width: `${progress?.percent || 5}%` }}
                />
              </div>
            </div>
          )}

          {updateInfo.release_notes && (
            <div className="pt-2 border-t border-white/5 text-[11px] text-zinc-400 max-h-32 overflow-y-auto whitespace-pre-line leading-relaxed">
              {updateInfo.release_notes}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
