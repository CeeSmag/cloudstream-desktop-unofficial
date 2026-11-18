import React from 'react';
import { Preferences, SourceInfo } from '../types';
import { filterSourcesBySubtitles } from '../utils/sourceFilter';
import {
  countByTier,
  filterSourcesByResolution,
  ResolutionTier,
  sortBySizeDescending,
} from '../utils/resolutionFilter';

/** What the Sources menu shows and how its filters are set. */
export interface SourceFilters {
  /** The sources to list: both filters applied, biggest files first while a resolution is picked. */
  visibleSources: SourceInfo[];
  subtitleFilter: string[];
  toggleSubtitle: (code: string) => void;
  resolutionFilter: ResolutionTier[];
  toggleResolution: (tier: ResolutionTier) => void;
  /** How many sources each filter alone keeps (for the counts next to each filter). */
  subtitleCount: number;
  resolutionCount: number;
  tierCounts: Record<ResolutionTier, number>;
}

function toggled<T>(list: T[], item: T): T[] {
  return list.includes(item) ? list.filter((entry) => entry !== item) : [...list, item];
}

/** The two filters of the Sources menu (subtitle languages, resolution) and the list they leave. */
export function useSourceFilters(sources: SourceInfo[], preferences: Preferences | null): SourceFilters {
  const [subtitleFilter, setSubtitleFilter] = React.useState<string[]>([]);
  const [resolutionFilter, setResolutionFilter] = React.useState<ResolutionTier[]>([]);
  const bySubtitles = filterSourcesBySubtitles(sources, preferences, subtitleFilter);
  const matching = filterSourcesByResolution(bySubtitles, resolutionFilter);
  return {
    // With a resolution picked the biggest files come first (sources without a known size follow).
    visibleSources: resolutionFilter.length > 0 ? sortBySizeDescending(matching) : matching,
    subtitleFilter,
    toggleSubtitle: (code) => setSubtitleFilter((current) => toggled(current, code)),
    resolutionFilter,
    toggleResolution: (tier) => setResolutionFilter((current) => toggled(current, tier)),
    subtitleCount: bySubtitles.length,
    resolutionCount: filterSourcesByResolution(sources, resolutionFilter).length,
    tierCounts: countByTier(sources),
  };
}
