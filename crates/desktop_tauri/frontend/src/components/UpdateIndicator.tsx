import React from 'react';
import { ArrowUpCircle, CheckCircle2, Loader2, AlertCircle } from 'lucide-react';
import { api } from '../services/api';
import { UpdateCheckResult } from '../types';

interface UpdateIndicatorProps {
  onOpenSettings: () => void;
}

type State =
  | { kind: 'checking' }
  | { kind: 'failed' }
  | { kind: 'done'; result: UpdateCheckResult };

/** Checks GitHub once per app launch and shows the result next to the provider pill. */
export const UpdateIndicator: React.FC<UpdateIndicatorProps> = ({ onOpenSettings }) => {
  const [state, setState] = React.useState<State>({ kind: 'checking' });

  React.useEffect(() => {
    let cancelled = false;
    api
      .checkForUpdates()
      .then((result) => !cancelled && setState({ kind: 'done', result }))
      .catch(() => !cancelled && setState({ kind: 'failed' }));
    return () => {
      cancelled = true;
    };
  }, []);

  let icon: React.ReactNode;
  let title: string;
  let cls = 'text-zinc-400 border-white/10 bg-white/5 hover:bg-white/10';
  let label: string | null = null;

  if (state.kind === 'checking') {
    icon = <Loader2 className="w-3.5 h-3.5 animate-spin" />;
    title = 'Checking for updates...';
  } else if (state.kind === 'failed') {
    icon = <AlertCircle className="w-3.5 h-3.5" />;
    title = 'Update check failed';
  } else if (state.result.has_update) {
    icon = <ArrowUpCircle className="w-3.5 h-3.5 animate-pulse" />;
    title = `Update available: v${state.result.latest_version}`;
    label = `v${state.result.latest_version}`;
    cls = 'text-accent border-accent/30 bg-accent/15 hover:bg-accent/25';
  } else {
    icon = <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />;
    title = `Up to date (v${state.result.current_version})`;
  }

  return (
    <button
      onClick={onOpenSettings}
      title={title}
      aria-label={title}
      className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl border text-xs font-semibold transition-colors ${cls}`}
    >
      {icon}
      {label && <span className="hidden sm:inline">{label}</span>}
    </button>
  );
};
