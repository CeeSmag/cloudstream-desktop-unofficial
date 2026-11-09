import React from 'react';
import { ShieldCheck, ArrowRight } from 'lucide-react';

interface CloudflarePromptProps {
  /** The site's host, as Cloudflare names it. */
  host: string;
  /** The provider that scrapes the site, when known. */
  provider: string | null;
  busy: boolean;
  /** The viewer says go: the check window may open. */
  onVerify: () => void;
}

/** Home's message when a site needs Cloudflare's "verify you are human" check before it can load. */
export const CloudflarePrompt: React.FC<CloudflarePromptProps> = ({ host, provider, busy, onVerify }) => {
  const name = provider ?? host;
  return (
    <div className="flex justify-center py-12 sm:py-20">
      <div className="w-full max-w-xl rounded-2xl border border-white/10 bg-surface-elevated p-6 sm:p-8 text-center">
        <div className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-full bg-accent/15 border border-accent/30">
          <ShieldCheck className="h-7 w-7 text-accent" />
        </div>
        <h2 className="text-xl sm:text-2xl font-bold text-white tracking-tight">{name} needs a quick check</h2>
        <p className="mt-3 text-sm sm:text-base text-zinc-400 leading-relaxed">
          {host} is protected by Cloudflare, which first checks that you are a person. CloudStream shows the check here
          for that. It usually takes a second and passes by itself; if it shows a box, tick it.
        </p>
        <button
          onClick={onVerify}
          disabled={busy}
          className="mt-6 inline-flex items-center gap-2 rounded-xl bg-accent px-5 py-2.5 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-accent/90 disabled:opacity-60"
        >
          {busy ? 'Opening the check...' : `Verify ${name}`}
          <ArrowRight className="h-4 w-4" />
        </button>
        <p className="mt-5 text-xs text-zinc-500">The check closes by itself when it is passed.</p>
      </div>
    </div>
  );
};
