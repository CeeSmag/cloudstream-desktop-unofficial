import React from 'react';
import { Loader2, SkipForward } from 'lucide-react';
import { CheckProgress } from '../types';
import { checkDetail, checkPercent, checkTitle } from '../utils/checkText';

interface SourceCheckPanelProps {
  progress: CheckProgress;
  onSkip: () => void;
}

/** Shown over the player while the streams of a video are checked, with a way to start right away. */
export const SourceCheckPanel: React.FC<SourceCheckPanelProps> = ({ progress, onSkip }) => {
  const canSkip = progress.total > 0 && !progress.skipping;
  const detail = checkDetail(progress);
  return (
    <div
      role="status"
      className="self-center mx-4 w-full max-w-sm flex flex-col items-center gap-3 p-6 rounded-2xl bg-midnight/80 backdrop-blur-md border border-white/10 shadow-2xl"
    >
      <Loader2 className="w-9 h-9 text-accent animate-spin" />
      <span className="text-sm font-medium text-zinc-100 text-center">{checkTitle(progress)}</span>
      {progress.total > 0 && (
        <div
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={checkPercent(progress)}
          className="w-full h-1.5 rounded-full bg-white/10 overflow-hidden"
        >
          <div
            className="h-full rounded-full bg-accent transition-all duration-200"
            style={{ width: `${checkPercent(progress)}%` }}
          />
        </div>
      )}
      {detail && <span className="text-xs text-zinc-400">{detail}</span>}
      {canSkip && (
        <button
          onClick={onSkip}
          className="mt-1 inline-flex items-center gap-2 px-4 py-2 rounded-xl bg-accent hover:bg-accent-hover text-white text-sm font-semibold transition-colors"
        >
          <SkipForward className="w-4 h-4" />
          Skip and play now
        </button>
      )}
    </div>
  );
};
