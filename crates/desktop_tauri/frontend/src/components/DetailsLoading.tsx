import React from 'react';
import { listen } from '@tauri-apps/api/event';
import { describeValue, logEvent } from '../utils/logger';
import { loadingLines, PluginProgress } from '../utils/loadingProgress';

/** How often a title that is still loading says so in the log. */
const LOG_EVERY_S = 10;

/** The spinner shown while a title page loads; after a few seconds it explains the wait and shows progress. */
export const DetailsLoading: React.FC = () => {
  const [waited, setWaited] = React.useState(0);
  const [progress, setProgress] = React.useState<PluginProgress | null>(null);

  React.useEffect(() => {
    const started = Date.now();
    const timer = window.setInterval(() => setWaited(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, []);

  React.useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<PluginProgress>('plugin-progress', (event) => setProgress(event.payload))
      .then((stop) => {
        unlisten = stop;
      })
      .catch((error: unknown) => {
        logEvent('error', `cannot listen for the plugin's loading progress: ${describeValue(error)}`);
      });
    return () => unlisten?.();
  }, []);

  React.useEffect(() => {
    if (waited > 0 && waited % LOG_EVERY_S === 0) {
      logEvent('info', `title still loading after ${waited} s; plugin progress ${JSON.stringify(progress)}`);
    }
  }, [waited, progress]);

  const lines = loadingLines(waited, progress);
  return (
    <div className="flex-1 flex flex-col items-center justify-center text-zinc-500 px-6 text-center">
      <div className="w-8 h-8 rounded-full border-2 border-accent border-t-transparent animate-spin mb-4" />
      <span className="text-sm">Loading details...</span>
      {lines.map((line) => (
        <p key={line} className="mt-2 max-w-md text-xs text-zinc-400">
          {line}
        </p>
      ))}
    </div>
  );
};
