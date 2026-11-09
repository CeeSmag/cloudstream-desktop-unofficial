import React from 'react';
import { X } from 'lucide-react';
import { CloudflarePrompt } from './CloudflarePrompt';

interface CloudflareAskProps {
  host: string;
  provider: string | null;
  busy: boolean;
  onVerify: () => void;
  onClose: () => void;
}

/** Shown over the app when something failed because a site is still waiting for the viewer's Verify. */
export const CloudflareAsk: React.FC<CloudflareAskProps> = ({ host, provider, busy, onVerify, onClose }) => (
  <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/70 p-4" role="dialog" aria-label="Verification needed">
    <div className="relative w-full max-w-xl">
      <button
        onClick={onClose}
        className="absolute right-3 top-3 z-10 rounded-lg p-1.5 text-zinc-400 hover:bg-white/10 hover:text-white"
        aria-label="Close"
      >
        <X className="h-5 w-5" />
      </button>
      <CloudflarePrompt host={host} provider={provider} busy={busy} onVerify={onVerify} />
    </div>
  </div>
);
