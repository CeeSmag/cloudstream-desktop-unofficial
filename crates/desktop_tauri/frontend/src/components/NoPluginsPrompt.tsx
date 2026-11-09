import React from 'react';
import { Puzzle, ArrowRight } from 'lucide-react';
import { HomeEmptyState } from '../utils/homeEmptyState';

interface NoPluginsPromptProps {
  state: Exclude<HomeEmptyState, 'none'>;
  /** Opens Settings on the Extensions page. */
  onOpenExtensions: () => void;
}

const STEPS = [
  'Open Settings, then Extensions.',
  'Add a repository: paste its link and confirm.',
  'Install one or more plugins from it. Your Home page fills up right away.',
];

/** Home's message when there is no plugin to build a feed from. */
export const NoPluginsPrompt: React.FC<NoPluginsPromptProps> = ({ state, onOpenExtensions }) => {
  const fresh = state === 'no-plugins';
  return (
    <div className="flex justify-center py-12 sm:py-20">
      <div className="w-full max-w-xl rounded-2xl border border-white/10 bg-surface-elevated p-6 sm:p-8 text-center">
        <div className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-full bg-accent/15 border border-accent/30">
          <Puzzle className="h-7 w-7 text-accent" />
        </div>
        <h2 className="text-xl sm:text-2xl font-bold text-white tracking-tight">
          {fresh ? 'Add a plugin to fill your Home page' : "Your plugins aren't showing up"}
        </h2>
        <p className="mt-3 text-sm sm:text-base text-zinc-400 leading-relaxed">
          {fresh
            ? "CloudStream doesn't come with any shows or movies of its own. Plugins bring them in, and your Home page is built from them."
            : 'You have plugins installed, but none of them is available right now. They may be switched off, or they failed to load.'}
        </p>
        {fresh && (
          <ol className="mt-5 space-y-2 text-left text-sm text-zinc-300">
            {STEPS.map((step, index) => (
              <li key={step} className="flex items-start gap-3">
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-white/10 text-xs font-semibold text-white">
                  {index + 1}
                </span>
                <span>{step}</span>
              </li>
            ))}
          </ol>
        )}
        <button
          onClick={onOpenExtensions}
          className="mt-6 inline-flex items-center gap-2 rounded-xl bg-accent px-5 py-2.5 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-accent/90"
        >
          {fresh ? 'Add plugins' : 'Open Extensions'}
          <ArrowRight className="h-4 w-4" />
        </button>
        {fresh && (
          <p className="mt-5 text-xs text-zinc-500">
            Plugins and repositories are made by other people, not by CloudStream. Only add ones you trust.
          </p>
        )}
      </div>
    </div>
  );
};
