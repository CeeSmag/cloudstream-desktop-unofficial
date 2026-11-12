import React from 'react';
import { RESOLUTION_TIERS, ResolutionTier } from '../utils/resolutionFilter';

interface ResolutionFilterProps {
  /** The toggled resolution groups. */
  toggled: ResolutionTier[];
  /** How many sources each group has. */
  counts: Record<ResolutionTier, number>;
  onToggle: (tier: ResolutionTier) => void;
  shown: number;
  total: number;
}

/** Toggles above the list of sources: only sources in a toggled resolution group are listed. */
export const ResolutionFilter: React.FC<ResolutionFilterProps> = ({ toggled, counts, onToggle, shown, total }) => (
  <div className="px-2 pb-2 pt-1 border-b border-white/10 mb-1">
    <div className="flex items-center justify-between px-1 pb-1.5 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
      <span>Resolution</span>
      <span className="font-medium normal-case tracking-normal text-zinc-500">
        {shown} of {total}
      </span>
    </div>
    <div className="flex flex-wrap gap-2">
      {RESOLUTION_TIERS.map((tier) => {
        const active = toggled.includes(tier.id);
        const empty = counts[tier.id] === 0 && !active;
        return (
          <button
            key={tier.id}
            onClick={() => onToggle(tier.id)}
            aria-pressed={active}
            title={`Only ${tier.id} sources (${tier.hint}): ${counts[tier.id]} here`}
            className={`px-2.5 py-1 rounded-lg border text-xs font-semibold transition-colors ${
              active
                ? 'bg-accent/20 border-accent text-accent'
                : 'bg-white/5 border-white/10 text-zinc-300 hover:bg-white/10'
            } ${empty ? 'opacity-40' : ''}`}
          >
            {tier.id} <span className="font-medium opacity-70">{counts[tier.id]}</span>
          </button>
        );
      })}
    </div>
    {toggled.length > 0 && shown === 0 && (
      <p className="px-1 pt-2 text-[11px] text-zinc-500">
        No source has this resolution yet. Sources that are still being checked show up here once they are.
      </p>
    )}
  </div>
);
