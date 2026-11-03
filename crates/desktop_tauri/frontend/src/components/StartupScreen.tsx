import React from 'react';
import { Film, TriangleAlert } from 'lucide-react';
import { StartupStatus } from '../types';
import { startupMessage, startupPercent } from '../utils/startupText';

interface StartupScreenProps {
  status: StartupStatus;
}

/** Full-window loading screen shown while the installed extensions are loaded. */
export const StartupScreen: React.FC<StartupScreenProps> = ({ status }) => {
  const percent = startupPercent(status);
  return (
    <div className="h-screen w-screen bg-midnight text-white flex flex-col items-center justify-center px-6 gap-6">
      <div className="w-16 h-16 rounded-2xl bg-accent flex items-center justify-center shadow-lg glow-accent">
        <Film className="w-8 h-8 text-white" />
      </div>
      <div className="text-center space-y-1 max-w-md">
        <h1 className="text-2xl font-bold tracking-tight">CloudStream</h1>
        <p role="status" className="text-sm text-zinc-400 break-words">
          {startupMessage(status)}
        </p>
      </div>
      <div
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        className="w-64 sm:w-80 h-1.5 rounded-full bg-white/10 overflow-hidden"
      >
        <div
          className={`h-full rounded-full bg-accent transition-all duration-300 ${status.total === 0 ? 'animate-pulse' : ''}`}
          style={{ width: `${status.total === 0 ? 100 : percent}%` }}
        />
      </div>
      {status.failures.length > 0 && (
        <p className="flex items-center gap-2 text-xs text-amber-300">
          <TriangleAlert className="w-4 h-4" />
          {status.failures.length} extension{status.failures.length === 1 ? '' : 's'} could not be loaded
        </p>
      )}
    </div>
  );
};
