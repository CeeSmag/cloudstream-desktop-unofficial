import React from 'react';
import { RefreshCw } from 'lucide-react';
import { SourceFilters } from '../hooks/useSourceFilters';
import { canRefreshSources, refreshHint } from '../utils/sourceRefresh';
import { LanguageFlag } from '../utils/subtitleFlags';
import { ResolutionFilter } from './ResolutionFilter';
import { SubtitleFilter } from './SubtitleFilter';

interface SourceMenuHeaderProps {
  filters: SourceFilters;
  /** The viewer's preferred subtitle languages, shown as toggles. */
  languages: LanguageFlag[];
  /** How many sources there are before any filter. */
  total: number;
  refresh: SourceRefresh;
}

/** The Refresh button's state and action. */
interface SourceRefresh {
  onRefresh: () => void;
  /** True while the video is still opening (its check panel shows). */
  checking: boolean;
  /** True while a refresh is starting. */
  refreshing: boolean;
}

/** Searches the sources again: the saved ones are forgotten and every source is checked anew. */
const RefreshButton: React.FC<SourceRefresh> = ({ onRefresh, checking, refreshing }) => {
  const enabled = canRefreshSources(checking, refreshing);
  return (
    <div className="px-2 pb-2 border-b border-white/10 mb-1">
      <button
        onClick={onRefresh}
        disabled={!enabled}
        title={refreshHint(checking, refreshing)}
        className={`w-full flex items-center justify-center gap-2 px-2.5 py-1.5 rounded-lg border text-xs font-semibold transition-colors bg-white/5 border-white/10 text-zinc-200 ${
          enabled ? 'hover:bg-white/10' : 'opacity-40 cursor-not-allowed'
        }`}
      >
        <RefreshCw className={`w-3.5 h-3.5 ${refreshing ? 'animate-spin' : ''}`} />
        Refresh sources
      </button>
    </div>
  );
};

/** Above the list of sources: the Refresh button, then the filters (resolution, subtitle languages). */
export const SourceMenuHeader: React.FC<SourceMenuHeaderProps> = ({ filters, languages, total, refresh }) => (
  <>
    <RefreshButton {...refresh} />
    <ResolutionFilter
      toggled={filters.resolutionFilter}
      counts={filters.tierCounts}
      onToggle={filters.toggleResolution}
      shown={filters.resolutionCount}
      total={total}
    />
    <SubtitleFilter
      languages={languages}
      toggled={filters.subtitleFilter}
      onToggle={filters.toggleSubtitle}
      shown={filters.subtitleCount}
      total={total}
    />
  </>
);
