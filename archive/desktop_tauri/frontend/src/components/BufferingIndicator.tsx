import React from 'react';
import { Loader2 } from 'lucide-react';

interface BufferingIndicatorProps {
  buffering: boolean;
  percent?: number | null;
}

/**
 * Top-middle buffering indicator displayed when stream loading stalls or pauses to fill cache.
 */
export const BufferingIndicator: React.FC<BufferingIndicatorProps> = ({ buffering, percent }) => {
  if (!buffering) return null;

  const hasPercent = percent !== null && percent !== undefined && percent >= 0 && percent <= 100;
  const label = hasPercent ? `Buffering ${Math.round(percent)}%` : 'Buffering...';

  return (
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none absolute top-4 sm:top-6 left-1/2 -translate-x-1/2 z-40 flex items-center gap-2.5 px-4 py-2 rounded-full bg-midnight/90 backdrop-blur-md border border-white/15 text-white shadow-2xl"
    >
      <Loader2 className="w-4 h-4 text-accent animate-spin" />
      <span className="text-xs sm:text-sm font-medium tracking-wide text-zinc-200">{label}</span>
    </div>
  );
};
