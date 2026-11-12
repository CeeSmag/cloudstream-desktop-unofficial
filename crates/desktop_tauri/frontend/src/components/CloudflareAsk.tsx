import React from 'react';
import { CloudflarePrompt } from './CloudflarePrompt';

interface CloudflareAskProps {
  host: string;
  provider: string | null;
  busy: boolean;
  onVerify: () => void;
  onClose: () => void;
}

/**
 * Shown over the app when something failed because a site is still waiting for the viewer's Verify.
 * It can always be closed: the X in the card's corner, the Escape key, or a click outside the card.
 */
export const CloudflareAsk: React.FC<CloudflareAskProps> = ({ host, provider, busy, onVerify, onClose }) => {
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-[90] flex items-center justify-center bg-black/70 p-4"
      role="dialog"
      aria-label="Verification needed"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <CloudflarePrompt host={host} provider={provider} busy={busy} onVerify={onVerify} onClose={onClose} padded={false} />
    </div>
  );
};
