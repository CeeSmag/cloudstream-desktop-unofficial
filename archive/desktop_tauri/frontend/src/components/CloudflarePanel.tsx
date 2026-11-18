import React from 'react';
import { listen } from '@tauri-apps/api/event';
import { ShieldCheck, X } from 'lucide-react';
import { api } from '../services/api';
import { usePanelPlacement } from '../hooks/usePanelPlacement';
import { describeValue, logEvent } from '../utils/logger';

interface PanelEvent {
  open: boolean;
  host: string;
}

/**
 * The Cloudflare check, inside the app: a frame the backend lays its browser view over. It opens when the
 * backend says a check is running and closes when it ends or the viewer cancels.
 */
export const CloudflarePanel: React.FC = () => {
  const [host, setHost] = React.useState<string | null>(null);
  const [problem, setProblem] = React.useState<string | null>(null);
  const frame = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<PanelEvent>('cloudflare-panel', (event) => {
      logEvent('info', `the backend ${event.payload.open ? 'opened' : 'closed'} the check frame for ${event.payload.host}`);
      setProblem(null);
      setHost(event.payload.open ? event.payload.host : null);
    })
      .then((stop) => {
        unlisten = stop;
      })
      .catch((error: unknown) => {
        logEvent('error', `cannot listen for the Cloudflare check frame: ${describeValue(error)}`);
      });
    return () => unlisten?.();
  }, []);

  usePanelPlacement(host, frame, setProblem);

  const open = host !== null;
  React.useEffect(() => {
    if (!open) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setHost(null);
        if (host) api.cancelCloudflarePanel(host).catch(() => undefined);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, host]);

  if (!host) return null;

  // The panel closes at once, whatever the backend answers: a check that has nothing left to cancel
  // (the site showed no challenge) must never leave the viewer stuck behind it.
  const cancel = () => {
    setHost(null);
    api.cancelCloudflarePanel(host).catch((error: unknown) => {
      logEvent('error', `cannot cancel the check for ${host}: ${describeValue(error)}`);
    });
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/70 p-4" role="dialog" aria-label={`Check ${host}`}>
      <div className="w-[min(92vw,760px)] rounded-2xl border border-white/10 bg-surface-elevated shadow-2xl">
        <div className="flex items-center gap-3 border-b border-white/10 px-4 py-3">
          <ShieldCheck className="h-5 w-5 text-accent" />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold text-white">Quick check for {host}</p>
            <p className="text-xs text-zinc-400">It passes by itself; if a box shows, tick it. This closes when done.</p>
          </div>
          <button
            onClick={cancel}
            className="inline-flex items-center gap-1 rounded-lg border border-white/10 px-3 py-1.5 text-xs text-zinc-300 hover:bg-white/10"
          >
            <X className="h-4 w-4" /> Cancel
          </button>
        </div>
        <div ref={frame} className="h-[min(66vh,600px)] w-full bg-black/40" data-testid="cloudflare-frame" />
        {problem && (
          <p className="border-t border-white/10 px-4 py-2 text-xs text-red-300" role="alert">
            The check could not be shown here: {problem}
          </p>
        )}
      </div>
    </div>
  );
};
