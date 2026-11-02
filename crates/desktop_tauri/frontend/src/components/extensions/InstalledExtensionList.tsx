import React from 'react';
import { ShieldCheck, ShieldAlert, Trash2 } from 'lucide-react';
import { InstalledExtension } from '../../types';

interface InstalledExtensionListProps {
  extensions: InstalledExtension[];
  busy: string | null;
  onUninstall: (internalName: string) => void;
  onSetTrusted: (internalName: string, trusted: boolean) => void;
}

/** The extensions installed on this computer, with trust and uninstall controls. */
export const InstalledExtensionList: React.FC<InstalledExtensionListProps> = ({
  extensions,
  busy,
  onUninstall,
  onSetTrusted,
}) => {
  const [confirming, setConfirming] = React.useState<string | null>(null);

  if (extensions.length === 0) {
    return <p className="text-xs text-zinc-500">No extensions installed yet. Browse a repository below to add one.</p>;
  }

  return (
    <div className="space-y-2">
      {extensions.map((extension) => {
        const name = extension.internal_name;
        const working = busy === `uninstall:${name}` || busy === `trust:${name}`;
        return (
          <div key={name} className="p-3.5 rounded-xl bg-surface-elevated/70 border border-white/5 space-y-2">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <h4 className="text-sm font-semibold text-white break-words">
                  {name} <span className="text-xs font-normal text-zinc-500">v{extension.version}</span>
                </h4>
                {extension.repository_url && (
                  <p className="text-xs text-zinc-500 font-mono break-all mt-0.5">{extension.repository_url}</p>
                )}
              </div>

              <div className="flex flex-none items-center gap-2">
                <button
                  onClick={() => onSetTrusted(name, !extension.trusted)}
                  disabled={working}
                  title="Trusted extensions may use files and system features. Applies the next time the app starts."
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition-colors disabled:opacity-50 ${
                    extension.trusted
                      ? 'bg-amber-500/15 border-amber-500/30 text-amber-300'
                      : 'bg-white/5 border-white/10 text-zinc-300 hover:bg-white/10'
                  }`}
                >
                  {extension.trusted ? <ShieldCheck className="w-3.5 h-3.5" /> : <ShieldAlert className="w-3.5 h-3.5" />}
                  <span>{extension.trusted ? 'Trusted' : 'Sandboxed'}</span>
                </button>

                {confirming === name ? (
                  <button
                    onClick={() => {
                      setConfirming(null);
                      onUninstall(name);
                    }}
                    onBlur={() => setConfirming(null)}
                    autoFocus
                    className="px-3 py-1.5 rounded-lg bg-rose-600 hover:bg-rose-500 text-white text-xs font-semibold"
                  >
                    Confirm
                  </button>
                ) : (
                  <button
                    onClick={() => setConfirming(name)}
                    disabled={working}
                    title="Uninstall"
                    className="p-2 rounded-lg text-zinc-400 hover:text-rose-400 hover:bg-white/5 transition-colors disabled:opacity-50"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                )}
              </div>
            </div>

            {extension.providers.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {extension.providers.map((provider) => (
                  <span key={provider} className="px-2 py-0.5 rounded-md bg-emerald-500/10 text-emerald-300 text-[11px]">
                    {provider}
                  </span>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
};
