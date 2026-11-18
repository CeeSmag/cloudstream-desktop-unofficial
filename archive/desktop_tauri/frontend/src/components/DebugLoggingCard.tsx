import React from 'react';
import { Bug, FolderOpen } from 'lucide-react';
import { useDebugLogging } from '../hooks/useDebugLogging';
import { formatFileSize } from '../utils/fileSize';

interface DebugLoggingCardProps {
  onError: (error: unknown) => void;
}

const LEVELS: { name: string; color: string; meaning: string }[] = [
  { name: 'ERROR', color: 'text-red-400', meaning: 'something failed' },
  { name: 'WARN', color: 'text-amber-300', meaning: 'odd, but the app carried on' },
  { name: 'INFO', color: 'text-sky-300', meaning: 'something normal happened' },
  { name: 'DEBUG', color: 'text-zinc-300', meaning: 'step by step detail' },
  { name: 'TRACE', color: 'text-zinc-500', meaning: 'finest detail' },
];

/** The switch for debug logging, with the folder and files it writes. */
export const DebugLoggingCard: React.FC<DebugLoggingCardProps> = ({ onError }) => {
  const { info, busy, toggle, openFolder } = useDebugLogging(onError);
  const enabled = info?.enabled ?? false;

  return (
    <div className="p-4 sm:p-6 rounded-2xl glass-panel space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 space-y-1">
          <h2 className="text-lg font-bold text-white flex items-center gap-2">
            <Bug className="w-5 h-5 text-accent flex-none" />
            <span>Debug logging</span>
          </h2>
          <p className="text-xs text-zinc-400">
            Writes detailed logs to separate text files, one for each part of the app, so odd behavior can be traced. Off by
            default: while it is off nothing is written and the app does no extra logging work. The player's own detailed messages (what it downloads, where it seeks, which pieces of a stream fail) go into the player file while it is on.
          </p>
        </div>
        <button
          role="switch"
          aria-checked={enabled}
          aria-label="Debug logging"
          disabled={busy || info === null}
          onClick={() => void toggle(!enabled)}
          className={`relative flex-none w-12 h-7 rounded-full transition-colors disabled:opacity-60 ${
            enabled ? 'bg-accent' : 'bg-white/15'
          }`}
        >
          <span
            className={`absolute top-1 left-1 w-5 h-5 rounded-full bg-white shadow transition-transform ${
              enabled ? 'translate-x-5' : ''
            }`}
          />
        </button>
      </div>

      {info && (
        <div className="space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
            <span className="text-zinc-400 break-all min-w-0">{info.folder}</span>
            <button
              onClick={() => void openFolder()}
              className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg bg-white/10 hover:bg-white/20 text-white font-medium"
            >
              <FolderOpen className="w-4 h-4" />
              Open folder
            </button>
          </div>

          <div className="space-y-1 text-xs">
            {info.files.map((file) => (
              <div key={file.file_name} className="flex justify-between gap-4 py-1 border-b border-white/5">
                <span className="min-w-0">
                  <span className="text-white font-medium">{file.file_name}</span>
                  <span className="text-zinc-500"> · {file.label}</span>
                </span>
                <span className="flex-none text-zinc-400">{formatFileSize(file.size_bytes)}</span>
              </div>
            ))}
          </div>

          <div className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-zinc-500">
            {LEVELS.map((level) => (
              <span key={level.name}>
                <span className={`font-semibold ${level.color}`}>{level.name}</span> {level.meaning}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
